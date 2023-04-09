//! Handle reading of markdown content.
use super::{fa_link, html, link_data, summary, UpdateInfo};
use crate::server::language;
use anyhow::{anyhow, Result};
use i18n_embed_fl::fl;
use lazy_regex::{regex_captures, regex_find, regex_replace_all};
use pulldown_cmark::{
    BrokenLink, CowStr, Event, HeadingLevel, Options, Parser, Tag,
};
use std::collections::BTreeMap;

pub(super) fn extract_metadata(src: &str) -> (BTreeMap<&str, &str>, &str) {
    let mut meta = BTreeMap::new();
    let mut src = src;
    while let Some((k, v, reminder)) =
        src.split_once('\n').and_then(|(line, reminder)| {
            line.split_once(':').map(|(k, v)| (k, v, reminder))
        })
    {
        meta.insert(k.trim(), v.trim());
        src = reminder;
    }
    (meta, src.trim())
}

pub(super) fn extract_parts(
    year: i16,
    slug: &str,
    lang: &str,
    markdown: &str,
    update: Option<&UpdateInfo>,
    files: &[(String, String)],
    loader: &mut super::Loader,
) -> Result<(String, String, String, Option<String>, String, bool)> {
    let (title, body, description) = Ctx::new(markdown, year, lang)
        .with_files(files)
        .md_to_html(loader)?;
    // Split at "more" marker, or try to find a good place if the text is long.
    let end = markdown.find("<!-- more -->").or_else(|| {
        let h1end = markdown.find('\n').unwrap_or(0);
        if markdown.len() < h1end + 900 {
            None
        } else {
            let mut end = h1end + 600;
            while !markdown.is_char_boundary(end) {
                end -= 1;
            }
            markdown[h1end..end].find("\n#").map(|e| h1end + e).or_else(
                || {
                    let e2 = h1end
                        + markdown[h1end..end].rfind("\n\n").unwrap_or(0);
                    if e2 > h1end + 50 {
                        Some(e2)
                    } else {
                        markdown[e2 + 2..]
                            .find("\n\n")
                            .map(|extra| e2 + 2 + extra)
                    }
                },
            )
        }
    });
    let (teaser, description) = if let Some(teaser) =
        end.map(|e| &markdown[..e])
    {
        // If teaser don't have an image and there is a "front" image ...
        let mut teaser = if let Some(img) = (!teaser.contains("\n!["))
            .then_some(())
            .and_then(|()| {
                regex_find!(
                    r#"!\[[^\]]*\]\[[^\]\{]*\{[^\}]*\bfront\b[^\}]*\}[^\]]*\]"#s,
                    markdown
                )
            })
        {
            // ... try to put the front image directly after the header.
            let s = teaser.find("\n\n").map_or(0, |s| s + 1);
            format!(
                "{}\n{}\n{}",
                &teaser[..s],
                img.replace("gallery", "sidebar"),
                &teaser[s..],
            )
        } else {
            teaser.into()
        };
        let fluent = language::load(lang).unwrap();
        if let Some(update) = update {
            if !update.info.is_empty() {
                teaser.push_str("\n\n**");
                teaser.push_str(&fl!(
                    fluent,
                    "update-at",
                    date =
                        (&crate::models::DateTime::wrap(update.date.into()))
                ));
                teaser.push_str("** ");
                teaser.push_str(&update.info);
            }
        }
        let teaser = regex_replace_all!(
            r#"\]\(\#([a-z0-0_]+)\)"#,
            &teaser,
            |_, target| format!("](/{}/{}.{}#{})", year, slug, lang, target),
        );
        Ctx::new(&teaser, year, lang)
            .md_to_html(loader)
            .map(|(_, teaser, desc)| (teaser, desc))?
    } else {
        (body.clone(), description)
    };
    let front_image = regex_captures!(
        "<figure[^>]*><(?:a href|img[^>]src)=['\"]([^'\"]+)['\"]",
        &teaser
    )
    .map(|(_, url)| url.to_string());
    let use_leaflet = body.contains("function initmap()");
    Ok((title, teaser, body, front_image, description, use_leaflet))
}

pub struct Ctx<'a> {
    markdown: &'a str,
    year: i16,
    lang: &'a str,
    files: &'a [(String, String)],
}
impl<'a> Ctx<'a> {
    pub fn new(markdown: &'a str, year: i16, lang: &'a str) -> Self {
        Ctx {
            markdown,
            year,
            lang,
            files: &[],
        }
    }
    pub fn with_files(mut self, files: &'a [(String, String)]) -> Self {
        self.files = files;
        self
    }
    fn fixlink(&self, link: BrokenLink) -> Option<(CowStr, CowStr)> {
        let reff = link.reference.trim_matches('`');
        self.files
            .iter()
            .find(|f| f.0 == reff)
            .map(|(_name, url)| (url.clone().into(), "".into()))
            .or_else(|| fa_link(reff).map(|url| (url.into(), "".into())))
            .or_else(|| {
                link_ext(&link, self.markdown, self.lang)
                    .map(|(url, title)| (url.into(), title.into()))
            })
            .or_else(|| Some((link.reference.to_string().into(), "".into())))
    }

    /// Convert my flavour of markdown to my preferred html.
    ///
    /// Returns the title and full content html markup separately.
    pub(super) fn md_to_html(
        &self,
        loader: &mut super::Loader,
    ) -> Result<(String, String, String)> {
        let mut fixlink = |broken_link: BrokenLink| self.fixlink(broken_link);
        let mut items = Parser::new_with_broken_link_callback(
            self.markdown,
            Options::all(),
            Some(&mut fixlink),
        )
        .collect::<Vec<_>>();

        let prefix = items_until(
            &mut items,
            &Event::Start(Tag::Heading(HeadingLevel::H1, None, vec![])),
        )
        .ok_or_else(|| anyhow!("No start of h1"))?;
        anyhow::ensure!(prefix.is_empty(), "Unexpected prefix: {:?}", prefix);

        let title = items_until(
            &mut items,
            &Event::End(Tag::Heading(HeadingLevel::H1, None, vec![])),
        )
        .ok_or_else(|| anyhow!("No end of h1"))?;

        let title = html::collect(title, loader, self.year, self.lang)?;
        let summary = summary::collect(items.clone().into_iter())?;
        let body =
            html::collect(items.into_iter(), loader, self.year, self.lang)?;
        Ok((title, body, summary))
    }
}

fn items_until<'a>(
    all: &mut Vec<Event<'a>>,
    delimiter: &Event,
) -> Option<Vec<Event<'a>>> {
    let pos = all.iter().position(|e| e == delimiter)?;
    let mut prefix = all.split_off(pos + 1);
    std::mem::swap(all, &mut prefix);
    prefix.pop(); // get rid of the delimiter itself
    Some(prefix)
}

fn link_ext(
    link: &BrokenLink,
    source: &str,
    lang: &str,
) -> Option<(String, String)> {
    let (_all, text, kind, _, attr_0, _, attrs) = regex_captures!(
        r"^\[(.*)\]\[(\w+)(:(\w+))?([,\s]+(.*))?\]$"s,
        &source[link.span.clone()],
    )?;
    let text = &regex_replace_all!(r"\s+", text, |_| " ");
    link_data(kind, text, attr_0, attrs, lang)
}
