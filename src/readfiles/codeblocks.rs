use super::Loader;
use crate::models::MyLang;
use anyhow::{bail, Result};
use base64::prelude::*;
use i18n_embed_fl::fl;
use pulldown_cmark_escape::escape_html;
use qr_code::QrCode;
use serde::Deserialize;
use std::{collections::BTreeMap, fmt::Write, sync::LazyLock, time::Instant};
use tree_sitter_collection::{
    tree_sitter::QueryError,
    tree_sitter_highlight::{
        Highlight, HighlightConfiguration, HighlightEvent, Highlighter,
    },
};

pub(super) trait BlockHandler {
    fn push(&mut self, content: &str) -> Result<()>;
    fn end(self) -> Result<()>;
}

pub(super) enum DynBlock<'a> {
    Leaflet(LeafletHandler<'a>),
    Code(CodeBlock<'a>),
    Qr(QrHandler<'a>),
    Embed(EmbedHandler<'a>),
}
impl<'a> DynBlock<'a> {
    pub fn for_kind(
        out: &'a mut String,
        fence: Option<&'a str>,
        loader: &'a mut Loader,
        year: i16,
        lang: MyLang,
    ) -> Result<DynBlock<'a>> {
        match fence.and_then(|l| {
            l.strip_prefix('!')
                .map(|l| l.split_once(' ').unwrap_or((l, "")))
        }) {
            Some(("leaflet", "")) => {
                Ok(DynBlock::Leaflet(LeafletHandler::open(out)))
            }
            Some(("qr", caption)) => {
                Ok(DynBlock::Qr(QrHandler::open(out, caption)))
            }
            Some(("embed", "")) => Ok(DynBlock::Embed(EmbedHandler::open(
                out, loader, year, lang,
            ))),
            Some((bang, _)) => {
                bail!("Magic for !{:?} not implemented", bang);
            }
            None => Ok(DynBlock::Code(CodeBlock::open(out, fence)?)),
        }
    }
}

impl BlockHandler for DynBlock<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        match self {
            DynBlock::Leaflet(x) => x.push(content),
            DynBlock::Code(x) => x.push(content),
            DynBlock::Qr(x) => x.push(content),
            DynBlock::Embed(x) => x.push(content),
        }
    }
    fn end(self) -> Result<()> {
        match self {
            DynBlock::Leaflet(x) => x.end(),
            DynBlock::Code(x) => x.end(),
            DynBlock::Qr(x) => x.end(),
            DynBlock::Embed(x) => x.end(),
        }
    }
}

pub struct LeafletHandler<'a> {
    out: &'a mut String,
}

impl LeafletHandler<'_> {
    fn open(out: &mut String) -> LeafletHandler {
        out.push_str(r#"
<div id="llmap">
<p>There should be a map here.</p>
</div>
<script type="text/javascript">
  function initmap() {
  var map = L.map('llmap', {scrollWheelZoom: false})
  .addLayer(L.tileLayer('//{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
  attribution: '&#xA9; <a href="http://osm.org/copyright">OpenStreetMaps bidragsgivare</a>',
  }));
"#);
        LeafletHandler { out }
    }
}
impl BlockHandler for LeafletHandler<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        self.out.push_str(content);
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.out.push_str("}\n</script>\n");
        Ok(())
    }
}

pub struct QrHandler<'a> {
    out: &'a mut String,
    caption: &'a str,
    data: String,
}

impl<'a> QrHandler<'a> {
    fn open(out: &'a mut String, caption: &'a str) -> Self {
        QrHandler {
            out,
            caption,
            data: String::new(),
        }
    }
}
impl BlockHandler for QrHandler<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        self.data.push_str(content);
        Ok(())
    }

    fn end(self) -> Result<()> {
        let qr = QrCode::new(self.data)?;
        let width = qr.width();
        let mut imgdata = Vec::new();
        let mut img = png::Encoder::new(&mut imgdata, width as _, width as _);
        img.set_color(png::ColorType::Grayscale);
        img.set_depth(png::BitDepth::One);
        img.set_compression(png::Compression::Best);
        let mut writer = img.write_header()?;
        writer.write_image_data(
            &qr.to_vec()
                .chunks(width)
                .flat_map(|line| {
                    line.chunks(8).map(|byte| {
                        byte.iter()
                            .fold(0, |acc, elem| 2 * acc + u8::from(!elem))
                            * (1 << (8 - byte.len()))
                    })
                })
                .collect::<Vec<_>>(),
        )?;
        writer.finish()?;

        let url = format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD_NO_PAD.encode(imgdata)
        );
        writeln!(
            self.out,
            "<figure class='qr-code sidebar'>\
             <img alt='' src='{url}' width='{width}' height='{width}'/>"
        )?;
        if !self.caption.is_empty() {
            self.out.push_str("<figcaption>");
            escape_html(&mut *self.out, self.caption)?;
            self.out.push_str("</figcaption>");
        }
        self.out.push_str("</figure>");
        Ok(())
    }
}

pub struct EmbedHandler<'a> {
    out: &'a mut String,
    data: String,
    loader: &'a mut Loader,
    year: i16,
    lang: MyLang,
}

impl<'a> EmbedHandler<'a> {
    fn open(
        out: &'a mut String,
        loader: &'a mut Loader,
        year: i16,
        lang: MyLang,
    ) -> Self {
        EmbedHandler {
            out,
            data: String::new(),
            loader,
            year,
            lang,
        }
    }
}
impl BlockHandler for EmbedHandler<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        self.data.push_str(content);
        Ok(())
    }

    fn end(self) -> Result<()> {
        let data = self.data.trim();
        if let Some(ytid) = data.strip_prefix("https://youtu.be/") {
            let id = format!("yt-{ytid}");
            let embed: EmbedData = self
                .loader
                .web
                .get("https://www.youtube.com/oembed")
                .query(&[("url", data), ("format", "json")])
                .send()?
                .error_for_status()?
                .json()?;
            let img = embed.thumbnail_url;
            let img = self
                .loader
                .fetch_content(
                    &img.replace("hqdefault.jpg", "maxresdefault.jpg"),
                )
                .or_else(|_| self.loader.fetch_content(&img))?;
            let img = self.loader.store_asset(
                self.year,
                &format!("{id}.jpg"),
                &img.0,
                &img.1,
            )?;
            let notice = fl!(self.lang.fluent(), "consent-youtube");
            writeln!(
                self.out,
                "<figure id='{id}' class='wrapiframe' \
                 style='padding-bottom: {aspect}%'>\
                 \n  <figcaption>{title}</figcaption>\
                 \n  <img class='ifrprev' src='{img}' \
                 width='{width}' height='{height}'>\
                 \n  <div class='ifrprev'><button \
                 onclick='document.getElementById(\"{id}\")\
                 .innerHTML=\"{iframe}\"'>⏵ Play</button>\
                 \n  <p>{notice}</p></div>\
                 </figure>",
                title = embed.title,
                aspect =
                    100. * f64::from(embed.height) / f64::from(embed.width),
                height = embed.height,
                width = embed.width,
                iframe = embed
                    .html
                    .replace("?feature=oembed", "?autoplay=1")
                    .replace('\'', "\\\'")
                    .replace('"', "\\\""),
            )?;
        } else {
            bail!("Unknown embed: {data:?}");
        }
        Ok(())
    }
}

/// The interesting parts of an oembed response.
#[derive(Debug, Deserialize)]
struct EmbedData {
    title: String,
    height: u32,
    width: u32,
    thumbnail_url: String,
    html: String,
}

pub struct CodeBlock<'a> {
    out: &'a mut String,
    lang: Option<&'static Lang>,
    code: bool,
    source: String,
}
impl<'a> CodeBlock<'a> {
    fn open(
        out: &'a mut String,
        lang: Option<&'a str>,
    ) -> Result<CodeBlock<'a>> {
        out.push_str("<pre");
        if let Some(lang) = lang {
            out.push_str(" data-lang=\"");
            escape_html(&mut *out, lang)?;
            out.push('"');
        }
        out.push('>');
        if lang.is_some() {
            out.push_str("<code>");
        }
        Ok(CodeBlock {
            out,
            lang: lang.and_then(|l| LANGS.get(l)),
            code: lang.is_some(),
            source: String::new(),
        })
    }
}
impl BlockHandler for CodeBlock<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        if self.lang.is_some() {
            self.source.push_str(content)
        } else {
            escape_html(&mut *self.out, content)?;
        }
        Ok(())
    }
    fn end(self) -> Result<()> {
        if let Some(conf) = self.lang {
            let mut highlighter = Highlighter::new();
            let highlights = highlighter.highlight(
                conf,
                self.source.as_bytes(),
                None,
                |lang_name| {
                    let res: Option<&Lang> = LANGS.get(lang_name);
                    match &res {
                        Some(_) => {
                            tracing::trace!("💉 Injecting {lang_name}")
                        }
                        None => tracing::trace!(
                            "No language found for {lang_name} injection"
                        ),
                    }
                    res
                },
            )?;

            for highlight in highlights {
                let highlight = highlight.unwrap();
                match highlight {
                    HighlightEvent::Source { start, end } => {
                        tracing::trace!(
                            "Escaping code from {start} to {end}"
                        );
                        escape_html(&mut *self.out, &self.source[start..end])
                            .unwrap();
                    }
                    HighlightEvent::HighlightStart(Highlight(i)) => {
                        tracing::trace!(
                            "Starting highlight {} (.hh{i})",
                            HIGHLIGHT_NAMES[i]
                        );
                        write!(&mut *self.out, r#"<i class=hh{i}>"#).unwrap();
                    }
                    HighlightEvent::HighlightEnd => {
                        tracing::trace!("Ending highlight");
                        write!(&mut *self.out, r#"</i>"#).unwrap();
                    }
                }
            }
        } else {
            self.out.push_str(&self.source);
        }
        if self.code {
            self.out.push_str("</code>");
        }
        self.out.push_str("</pre>\n");
        Ok(())
    }
}

use HighlightConfiguration as Lang;

static LANGS: LazyLock<LangFinder> = LazyLock::new(LangFinder::new);

#[derive(Default)]
struct LangFinder {
    known: BTreeMap<&'static str, &'static Lang>,
}

impl LangFinder {
    fn new() -> Self {
        let start = Instant::now();
        let mut slf = Self {
            known: BTreeMap::new(),
        };
        use tree_sitter_collection as tsc;
        slf.add("css", tsc::css);
        slf.add("html", tsc::html);
        slf.add("json", tsc::javascript);
        slf.add("markdown", tsc::markdown);
        slf.known.insert("md", slf.known.get("markdown").unwrap());
        slf.add("scss", tsc::scss);
        slf.add("bash", tsc::bash);
        // Note: Not sure about this, a shell session also contains output!
        // Fasterthanlime somehow uses ascii escapes from shell session!
        slf.known.insert("sh", slf.known.get("bash").unwrap());
        slf.add("rust", tsc::rust);
        tracing::info!(elapsed = ?start.elapsed(), "Loaded language defs");
        slf
    }
    fn get(&self, name: &str) -> Option<&Lang> {
        self.known.get(name).copied().or_else(|| {
            tracing::info!("Language {name:?} not found");
            None
        })
    }

    fn add(
        &mut self,
        name: &'static str,
        build: impl Fn() -> Result<Lang, QueryError>,
    ) {
        let mut conf = build()
            .map_err(|e| format!("Failed to build {name}: {e}"))
            .unwrap();
        conf.configure(HIGHLIGHT_NAMES);
        self.known.insert(name, Box::leak(Box::new(conf)));
    }
}

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant",
    "function.builtin",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "comment",
    "macro",
    "label",
    "diff.addition",
    "diff.deletion",
    // markdown_inline
    "number",
    "text.literal",
    "text.emphasis",
    "text.strong",
    "text.uri",
    "text.reference",
    "string.escape",
    // markdown
    "text.title",
    "punctuation.special",
    "text.strikethrough",
];
