use super::Loader;
use crate::models::MyLang;
use crate::syntax_hl::ClassedHTMLGenerator;
use crate::syntax_hl::LinesWithEndings;
use anyhow::{bail, Result};
use base64::prelude::*;
use i18n_embed_fl::fl;
use pulldown_cmark_escape::escape_html;
use qr_code::QrCode;
use serde::Deserialize;
use std::fmt::Write;

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
    gen: Option<ClassedHTMLGenerator<'a>>,
    code: bool,
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
            gen: lang.and_then(crate::syntax_hl::for_lang),
            code: lang.is_some(),
        })
    }
}
impl BlockHandler for CodeBlock<'_> {
    fn push(&mut self, content: &str) -> Result<()> {
        if let Some(gen) = &mut self.gen {
            for line in LinesWithEndings::from(content) {
                gen.parse_html_for_line_which_includes_newline(line)?;
            }
        } else {
            escape_html(&mut *self.out, content)?;
        }
        Ok(())
    }
    fn end(self) -> Result<()> {
        if let Some(gen) = self.gen {
            self.out.push_str(&gen.finalize());
        }
        if self.code {
            self.out.push_str("</code>");
        }
        self.out.push_str("</pre>\n");
        Ok(())
    }
}
