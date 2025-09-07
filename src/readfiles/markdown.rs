//! Handle reading of markdown content.
use super::{fa_link, html, link_data, summary, PageRef, UpdateInfo};
use super::{DateTime, Loader};
use crate::models::MyLang;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, Local};
use i18n_embed_fl::fl;
use lazy_regex::{regex_captures, regex_replace_all};
use pulldown_cmark::{
    BrokenLink, BrokenLinkCallback, CowStr, Event, Options, Parser, Tag,
    TagEnd,
};
use std::cell::OnceCell;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, info, warn};

pub struct Ctx<'a> {
    markdown: &'a str,
    slug: &'a str,
    lang: MyLang,
    files: OnceCell<Vec<(String, String)>>,
}
impl<'a> Ctx<'a> {
    pub fn new(markdown: &'a str, slug: &'a str, lang: MyLang) -> Self {
        Ctx {
            markdown,
            slug,
            lang,
            files: OnceCell::new(),
        }
    }

    pub fn parser(&self) -> Result<ContentParser<'_>> {
        let mut items = Parser::new_with_broken_link_callback(
            self.markdown,
            Options::all(),
            Some(self),
        );
        let meta = ContentMeta::load(&mut items)?;

        let year = if meta.is_meta {
            0
        } else {
            meta.pubdate
                .map_or_else(|| Local::now().year(), |d| d.year())
                .try_into()?
        };
        Ok(ContentParser {
            ctx: self,
            year,
            items,
            meta,
        })
    }

    pub fn set_files(&self, files: Vec<(String, String)>) -> Result<()> {
        self.files
            .set(files.clone())
            .map_err(|f| anyhow!("Set files {files:?} but was already {f:?}"))
    }
}

impl<'input> BrokenLinkCallback<'input> for &'input Ctx<'input> {
    fn handle_broken_link(
        &mut self,
        link: BrokenLink<'input>,
    ) -> Option<(CowStr<'input>, CowStr<'input>)> {
        let reff = link.reference.trim_matches('`');
        if let Some(files) = self.files.get() {
            if let Some((_, url)) = files.iter().find(|f| f.0 == reff) {
                return Some((url.clone().into(), "".into()));
            }
        } else {
            warn!("Files not set yet!");
        }
        fa_link(reff)
            .map(|url| (url.into(), "".into()))
            .or_else(|| {
                link_ext(&link, self.markdown, self.lang)
                    .map(|(url, title)| (url.into(), title.into()))
            })
            .or_else(|| Some((link.reference.to_string().into(), "".into())))
    }
}

pub struct ContentParser<'src> {
    ctx: &'src Ctx<'src>,
    pub year: i16,
    items: Parser<'src, &'src Ctx<'src>>,
    meta: ContentMeta,
}

impl ContentParser<'_> {
    pub fn meta(&self) -> &ContentMeta {
        &self.meta
    }

    pub(super) fn load_assets(
        &self,
        path: &Path,
        loader: &mut Loader,
    ) -> Result<()> {
        let files = self
            .meta
            .files()
            .map(|s| {
                info!("Loading assed data for {s:?}");
                loader
                    .handle_asset(path, s, self.year)
                    .with_context(|| format!("Asset {s:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.ctx.set_files(files)?;
        Ok(())
    }

    pub(super) fn load_title(
        &mut self,
        loader: &mut Loader,
    ) -> Result<String> {
        match self.items.next() {
            Some(Event::Start(Tag::Heading { .. })) => (),
            x => bail!("Expteded h1, got {x:?}"),
        }
        let mut events = vec![];
        for e in &mut self.items {
            if matches!(e, Event::End(TagEnd::Heading(_))) {
                break;
            }
            events.push(e);
        }
        let mut title = html::collect(events, loader, &self.get_url())?;
        if self.meta.pubdate.is_none() && !self.meta.is_meta {
            title.push_str(" \u{1f58b}");
        }
        Ok(title)
    }

    pub(super) fn into_html(self, loader: &mut Loader) -> Result<String> {
        let url = self.get_url();
        html::collect(self.items, loader, &url)
    }

    pub(super) fn get_url(&self) -> PageRef {
        PageRef {
            year: self.year,
            slug: self.ctx.slug.to_owned(),
            lang: self.ctx.lang.to_owned(),
        }
    }
}

#[derive(Default, Debug)]
pub struct ContentMeta {
    pub is_meta: bool,
    pub tags: Option<String>,
    pub pubdate: Option<DateTime>,
    pub(super) update: Option<UpdateInfo>,
    res: Option<String>,
}

impl ContentMeta {
    fn load<'a>(items: &mut impl Iterator<Item = Event<'a>>) -> Result<Self> {
        use pulldown_cmark::MetadataBlockKind as Kind;
        match items.next() {
            Some(Event::Start(Tag::MetadataBlock(Kind::YamlStyle))) => (),
            x => bail!("Expteded start of metadata, got {x:?}"),
        }
        let meta = match items.next() {
            Some(Event::Text(data)) => data.parse()?,
            x => bail!("Expteded metadata, got {x:?}"),
        };
        match items.next() {
            Some(Event::End(TagEnd::MetadataBlock(Kind::YamlStyle))) => (),
            x => bail!("Expteded end of metadata, got {x:?}"),
        }
        Ok(meta)
    }
}

impl FromStr for ContentMeta {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> std::result::Result<Self, Self::Err> {
        let mut result = Self::default();
        for line in data.lines() {
            let (key, value) = if let Some(i) = line.find(':') {
                (&line[0..i], line[i + 1..].trim())
            } else {
                (line, "")
            };
            match (key, value) {
                ("pubdate", v) => {
                    result.pubdate = Some(v.parse()?);
                }
                ("tags", v) => {
                    result.tags = Some(v.into());
                }
                ("res", v) => {
                    result.res = Some(v.into());
                }
                ("update", v) => result.update = Some(v.parse()?),
                ("meta", _) => result.is_meta = true,
                (k, v) => todo!("Handle metadata {k:?}; {v:?}"),
            }
        }
        Ok(result)
    }
}

impl ContentMeta {
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.res
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

pub struct Body {
    pub title: String,
    pub teaser: String,
    pub body: String,
    pub summary: String,
    pub front_image: Option<String>,
    pub use_leaflet: bool,
}

impl Body {
    pub fn load(
        mut data: ContentParser,
        loader: &mut Loader,
    ) -> Result<Self> {
        let title = data.load_title(loader)?;

        let url = data.get_url();
        let items = data.items.collect::<Vec<_>>();

        let mut find_img = items.iter().cloned();
        let front_image = if let Some((dest_url, title)) =
            find_img.find_map(|e| match e {
                Event::Start(Tag::Image {
                    dest_url, title, ..
                }) if img_is_front(&title, &dest_url) => {
                    Some((dest_url, title))
                }
                _ => None,
            }) {
            let mut html = String::new();
            html::write_image(
                &mut html,
                &dest_url,
                &title,
                loader,
                &mut find_img,
                false,
            )?;
            Some(html)
        } else {
            None
        };

        let body = html::collect(items.iter().cloned(), loader, &url)?;

        let (teaser, summary) =
            if let Some(teaser_items) = find_teaser(&items) {
                let mut teaser_extra = String::new();
                let extra_teaser = match &data.meta.update {
                    Some(update) if !update.info.is_empty() => {
                        let fluent = url.lang.fluent();
                        teaser_extra.push_str("\n\n**");
                        teaser_extra.push_str(&fl!(
                            fluent,
                            "update-at",
                            date = (&crate::models::DateTime::wrap(
                                update.date.into()
                            ))
                        ));
                        teaser_extra.push_str("** ");
                        teaser_extra.push_str(&update.info);

                        Parser::new_with_broken_link_callback(
                            &teaser_extra,
                            Options::all(),
                            Some(data.ctx),
                        )
                        .collect()
                    }
                    _ => vec![],
                };

                let teaser = html::collect(
                    teaser_items.iter().chain(&extra_teaser).cloned(),
                    loader,
                    &url,
                )?;
                let summary = summary::collect(
                    teaser_items.iter().cloned().chain(extra_teaser),
                )?;

                let teaser = front_image
                    .as_deref()
                    .and_then(|img| {
                        if teaser.contains(img) {
                            None
                        } else {
                            Some(format!("{img}\n{teaser}"))
                        }
                    })
                    .unwrap_or(teaser);

                (teaser, summary)
            } else {
                (body.clone(), summary::collect(items.iter().cloned())?)
            };

        let front_image = front_image
            .as_ref()
            .or(Some(&body))
            .and_then(|html| {
                regex_captures!(
                    "<figure[^>]*><(?:a href|img[^>]src)=['\"]([^'\"]+)['\"]",
                    html,
                )
            })
            .map(|(_, url)| url.to_string());

        let use_leaflet = body.contains("function initmap()");

        Ok(Self {
            title,
            teaser,
            body,
            summary,
            front_image,
            use_leaflet,
        })
    }
}

fn img_is_front(title: &str, dest_url: &str) -> bool {
    title.contains("front") || dest_url.contains("front")
}

fn find_teaser<'a>(all: &'a [Event<'a>]) -> Option<&'a [Event<'a>]> {
    all.iter()
        .position(|e| {
            matches!(e, Event::Html(s) if s.as_ref() == "<!-- more -->\n")
        })
        .map(|pos| pos - 1)
        .or_else(|| find_teaser_by_size(all))
        .map(|end| {
            debug!("Tesaser is {end} items out of {}", all.len());
            &all[..end]
        })
}

fn find_teaser_by_size<'a>(all: &'a [Event<'a>]) -> Option<usize> {
    let low_limit = 720;
    let high_limit = 1100;

    let mut weight = 0;
    let mut enumerated = all.iter().enumerate();

    while let Some((i, e)) = enumerated.next() {
        if weight > low_limit {
            debug!("Weight stop at {i} ({weight} before {e:?})");
            return Some(i - 1);
        }

        match e {
            Event::Start(Tag::Paragraph) => {
                let (_ii, mut w, has_img) =
                    inline_until(enumerated.by_ref(), TagEnd::Paragraph);
                w += 80;
                debug!("Paragraph at {i} ({has_img} after {weight}) is {w}");
                weight += w;
                let extra = if i > 0 && has_img { 400 } else { 0 };
                if weight + extra > high_limit {
                    return Some(i - 1);
                }
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                let (_ii, mut w, _) = inline_until(
                    enumerated.by_ref(),
                    TagEnd::BlockQuote(*kind),
                );
                w += 150;
                debug!("Blockquote at {i} (after {weight}) is {w}");
                weight += w;
                if weight > high_limit {
                    return Some(i - 1);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                let (_ii, mut w, _) =
                    inline_until(enumerated.by_ref(), TagEnd::CodeBlock);
                w += 100;
                debug!("Codeblock at {i} (after {weight}) is {w}");
                weight += w;
                if weight > high_limit {
                    return Some(i - 1);
                }
            }
            Event::Start(Tag::HtmlBlock) => {
                for (_ii, e) in enumerated.by_ref() {
                    match e {
                        Event::End(TagEnd::HtmlBlock) => {
                            break;
                        }
                        Event::Html(s) => {
                            weight += s.len() / 4;
                            if weight > high_limit {
                                debug!("Html block reached {weight}, stop.");
                                return Some(i - 1);
                            }
                        }
                        e => todo!("Handle {e:?} in html block"),
                    }
                }
            }
            Event::Start(Tag::List(_)) => {
                if weight + 200 > low_limit {
                    return Some(i - 1);
                }
            }
            Event::End(TagEnd::List(_)) => (), // i += 1),
            Event::Start(Tag::Item) => {
                let (_ii, mut w, _) =
                    inline_until(enumerated.by_ref(), TagEnd::Item);
                w += 30;
                debug!("Item at {i} (after {weight}) is {w}");
                weight += w;
                if weight > high_limit {
                    return Some(i - 1);
                }
            }

            // No sections or chapters in the teaser!
            Event::Start(Tag::Heading { .. }) => return Some(i - 1),

            e => todo!("Unexpected root-level at {i}: {e:?}"),
        }
    }
    None
}

fn inline_until<'a, I>(items: &mut I, end: TagEnd) -> (usize, usize, bool)
where
    I: Iterator<Item = (usize, &'a Event<'a>)>,
{
    let mut has_img = false;
    let mut weight = 0;
    while let Some((ii, e)) = items.next() {
        match e {
            Event::End(e) if *e == end => {
                return (ii, weight, has_img);
            }
            Event::Start(Tag::Paragraph) => {
                let (_, w, h_i) =
                    inline_until(items.by_ref(), TagEnd::Paragraph);
                has_img |= h_i;
                weight += w + 60;
            }
            Event::Text(s) => {
                weight += s.len();
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                has_img = true;
                if !img_is_front(title, dest_url) {
                    weight += 200;
                }
            }
            Event::InlineHtml(s) => weight += s.len() / 8,
            Event::Code(s) => weight += s.len() + 1,
            Event::SoftBreak => weight += 1,

            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Link { .. })
            | Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Link
                | TagEnd::Image,
            ) => (),

            e => todo!("Handle {e:?} in inline"),
        }
    }
    unreachable!("Inline ended before file");
}

fn link_ext(
    link: &BrokenLink,
    source: &str,
    lang: MyLang,
) -> Option<(String, String)> {
    let (_all, text, kind, _, attr_0, _, attrs) = regex_captures!(
        r"^\[(.*)\]\[(\w+)(:(\w+))?([,\s]+(.*))?\]$"s,
        &source[link.span.clone()],
    )?;
    let text = &regex_replace_all!(r"\s+", text, |_| " ");
    link_data(kind, text, attr_0, attrs, lang)
}
