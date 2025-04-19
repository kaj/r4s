//! How to serialize parsed markdown into my kind of html
use super::codeblocks::{BlockHandler, DynBlock};
use super::{FaRef, Loader, PageRef};
use crate::models::safe_md2html;
use anyhow::{bail, Context, Result};
use lazy_regex::regex_captures;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use pulldown_cmark_escape::{escape_href, escape_html};
use std::fmt::{self, Write};
use tracing::{info, warn};

pub(super) fn collect<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
    loader: &mut Loader,
    url: &PageRef,
) -> Result<String> {
    let mut result = String::new();
    let mut data = data.into_iter();
    let mut section_level = 1;
    while let Some(event) = data.next() {
        match event {
            Event::Text(text) => {
                escape_html(&mut result, &text)?;
            }
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs: _,
            }) => {
                {
                    let level = level as u32;
                    while section_level >= level {
                        result.push_str("</section>");
                        section_level -= 1;
                    }
                    result.push('\n');
                    while section_level + 1 < level {
                        result.push_str("<section>");
                        section_level += 1;
                    }
                }
                result.push_str("<section");
                if let Some(id) = id {
                    result.push_str(" id=\"");
                    escape_html(&mut result, &id)?;
                    result.push('"');
                }
                if !classes.is_empty() {
                    result.push_str(" class=\"");
                    escape_html(&mut result, &classes.join(" "))?;
                    result.push('"');
                }
                result.push('>');
                section_level += 1;
                result.push_str(&format!("<{}>", level));
            }
            Event::End(TagEnd::Heading(level)) => {
                if !remove_end(&mut result, &format!("<{}>", level)) {
                    result.push_str(&format!("</{}>\n", level));
                }
            }
            Event::Start(Tag::CodeBlock(blocktype)) => {
                let fence = match blocktype {
                    CodeBlockKind::Fenced(ref f) => {
                        (!f.is_empty()).then(|| f.as_ref())
                    }
                    CodeBlockKind::Indented => None,
                };
                let mut handler = DynBlock::for_kind(
                    &mut result,
                    fence,
                    loader,
                    url.year,
                    &url.lang,
                )?;
                for event in &mut data {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(code) => handler.push(&code)?,
                        x => bail!("Unexpeted in code: {:?}", x),
                    }
                }
                handler.end()?;
            }
            Event::End(TagEnd::CodeBlock) => {
                unreachable!();
            }
            Event::Start(Tag::Image {
                link_type: _,
                dest_url,
                title,
                id: _,
            }) => {
                write_image(
                    &mut result,
                    &dest_url,
                    &title,
                    loader,
                    &mut data,
                    true,
                )?;
            }
            Event::End(TagEnd::Paragraph)
                if result.ends_with("<p><!--no-p-->") =>
            {
                result.truncate(result.len() - 14);
            }
            Event::Start(Tag::TableHead) => {
                result.push_str("<thead><tr>");
            }
            Event::End(TagEnd::TableHead) => {
                result.push_str("</tr></thead>\n");
            }
            Event::TaskListMarker(done) => {
                result.push_str("<input disabled type='checkbox'");
                if done {
                    result.push_str(" checked=''");
                }
                result.push_str("/>\n");
            }
            // Content of htmlblock is Event::Html, below.
            Event::Start(Tag::HtmlBlock) => (),
            Event::End(TagEnd::HtmlBlock) => (),
            Event::Start(tag) => {
                result.push('<');
                result.push_str(tag_name(&tag));
                match tag {
                    Tag::Paragraph | Tag::Emphasis => (),
                    Tag::TableCell | Tag::TableRow => (),
                    Tag::List(None) => (),
                    Tag::List(Some(start)) => {
                        result.push_str(&format!(" start='{}'", start));
                    }
                    Tag::Item => (),
                    Tag::Link {
                        link_type: _,
                        dest_url,
                        title,
                        id,
                    } => {
                        if !dest_url.is_empty() {
                            result.push_str(" href=\"");
                            if dest_url.starts_with('#') {
                                info!("Got local link {dest_url:?}");
                                escape_href(&mut result, &url.to_string())?;
                            }
                            escape_href(&mut result, &dest_url)?;
                            result.push('"');
                        }
                        if !id.is_empty() {
                            result.push_str(" id=\"");
                            escape_html(&mut result, &id)?;
                            result.push('"');
                        }
                        if !title.is_empty() {
                            result.push_str(" title=\"");
                            escape_html(&mut result, &title)?;
                            result.push('"');
                        }
                    }
                    t => result.push_str(&format!("><!-- {:?} --", t)),
                }
                result.push('>');
            }
            Event::End(tag) => {
                result.push_str("</");
                result.push_str(tag_name_e(&tag));
                result.push('>');
                if matches!(
                    tag,
                    TagEnd::Paragraph
                        | TagEnd::Table
                        | TagEnd::Item
                        | TagEnd::List(_)
                ) {
                    // Maybe more?
                    result.push('\n');
                }
            }
            Event::Rule => result.push_str("<hr>\n"),
            Event::SoftBreak => result.push('\n'),
            Event::Html(code) => result.push_str(&code),
            Event::Code(code) => {
                if code.starts_with('[') && code.ends_with(']') {
                    result.push_str("<code class='key'>");
                    escape_html(
                        &mut result,
                        code.trim_start_matches('[').trim_end_matches(']'),
                    )?;
                    result.push_str("</code>");
                } else {
                    result.push_str("<code>");
                    escape_html(&mut result, &code)?;
                    result.push_str("</code>");
                }
            }
            Event::HardBreak => {
                result.push_str("<br/>\n");
            }
            Event::InlineHtml(code) => {
                warn!("Found raw html: {code:?}.");
                result.push_str(&code)
            }
            e => bail!("Unhandled: {:?}", e),
        }
    }
    for _ in 2..=section_level {
        result.push_str("</section>");
    }
    Ok(result)
}

pub fn write_image<'a>(
    result: &mut String,
    dest_url: &str,
    title: &str,
    loader: &mut Loader,
    data: &mut impl Iterator<Item = Event<'a>>,
    allow_gallery: bool,
) -> Result<()> {
    // TODO: Respect id.
    let _ = remove_end(result, "<p>")
        || remove_end(result, "<p><!--no-p-->")
        || remove_end(result, "<p><!--no-p-->\n");
    let mut inner = String::new();
    for tag in data {
        match tag {
            Event::End(TagEnd::Image) => break,
            Event::Text(text) => inner.push_str(&text),
            Event::SoftBreak => inner.push(' '),
            Event::Start(Tag::Emphasis | Tag::Strong) => (),
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => (),
            // Inner is mainly the alt, so no inline html.
            Event::InlineHtml(_) => (),
            _ => bail!("Unexpected {tag:?} in image"),
        }
    }

    let (imgref, classes, attrs, caption) = if !title.is_empty() {
        regex_captures!(
            r#"^(\{([\s\w]*)((?:\s[\w-]*="[^"]+")*)\})?\s*(.*)$"#,
            &title,
        )
        .map(|(_all, _, classes, attrs, caption)| {
            (dest_url, classes, attrs, safe_md2html(caption))
        })
        .with_context(|| format!("Bad image ref: {:?}", dest_url))?
    } else {
        tracing::warn!("Found old-format image.");
        regex_captures!(
            r#"^([A-Za-z0-9/._-]*)\s*(\{([\s\w]*)((?:\s[\w-]*="[^"]+")*)\})?\s*([^{]*)$"#m,
            &dest_url,
        )
            .map(|(_all, imgref, _, classes, attrs, caption)| (imgref, classes, attrs, caption.to_string()))
            .with_context(|| {
                format!("Bad image ref: {:?}", dest_url)
            })?
    };
    let mut classes = ClassList::from(classes);
    if !allow_gallery {
        classes.replace("gallery", "sidebar");
    }
    let do_gallery = allow_gallery && classes.has("gallery");
    if do_gallery && !remove_end(result, "</div><!--gallery-->\n") {
        result.push_str("<div class='gallery'>");
    }

    if imgref == "cover" {
        let url = inner.parse::<FaRef>().unwrap().cover();
        classes.add("fa-cover");
        writeln!(
            result,
            "<figure class='{}'>\
             <a href='{url}'><img alt='Omslagsbild {}' src='{url}' width='150'/></a>\
             <figcaption>{} {} {}</figcaption></figure>",
            classes, inner, inner, caption, title,
            url = url,
        )
                        .unwrap();
    } else {
        let imgdata = loader.imgcli.fetch(imgref)?;
        if !imgdata.is_public() {
            tracing::warn!("Image {:?} is not public", imgref);
        }
        let alt = inner.trim();
        let imgtag = if classes.has("scaled") {
            imgdata.markup_large(alt)
        } else {
            imgdata.markup(alt)
        };
        if imgdata.is_portrait() {
            classes.add("portrait");
        };
        writeln!(
            result,
            "<figure class='{classes}'{attrs}>{imgtag}\
             <figcaption>{caption}</figcaption></figure>",
        )
        .unwrap();
    }
    if allow_gallery {
        if do_gallery {
            result.push_str("</div><!--gallery-->\n");
        }
        result.push_str("<p><!--no-p-->");
    }
    Ok(())
}

struct ClassList<'a>(Vec<&'a str>);
impl<'a> From<&'a str> for ClassList<'a> {
    fn from(value: &'a str) -> Self {
        Self(value.split_ascii_whitespace().collect())
    }
}
impl<'a> ClassList<'a> {
    fn has(&self, cls: &str) -> bool {
        self.0.iter().any(|c| *c == cls)
    }
    fn add(&mut self, cls: &'a str) {
        self.0.push(cls);
    }
    fn replace(&mut self, from: &str, to: &'a str) {
        for cls in &mut self.0 {
            if *cls == from {
                *cls = to;
            }
        }
    }
}
impl fmt::Display for ClassList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((first, rest)) = self.0.split_first() {
            f.write_str(first)?;
            for c in rest {
                f.write_char(' ')?;
                f.write_str(c)?;
            }
        }
        Ok(())
    }
}

fn remove_end(s: &mut String, tail: &str) -> bool {
    if s.ends_with(tail) {
        s.truncate(s.len() - tail.len());
        true
    } else {
        false
    }
}

fn tag_name(tag: &Tag) -> &'static str {
    match tag {
        Tag::BlockQuote(..) => "blockquote",
        Tag::Emphasis => "em",
        Tag::Item => "li",
        Tag::Link { .. } => "a",
        Tag::List(None) => "ul",
        Tag::List(Some(_)) => "ol",
        Tag::Paragraph => "p",
        Tag::Strong => "strong",
        Tag::Table(..) => "table",
        Tag::TableCell => "td",
        Tag::TableRow => "tr",
        Tag::DefinitionList => "dl",
        Tag::DefinitionListTitle => "dt",
        Tag::DefinitionListDefinition => "dd",
        tag => panic!("Not a simple tag: {:?}", tag),
    }
}
fn tag_name_e(tag: &TagEnd) -> &'static str {
    match tag {
        TagEnd::BlockQuote(_) => "blockquote",
        TagEnd::Emphasis => "em",
        TagEnd::Item => "li",
        TagEnd::Link => "a",
        TagEnd::List(true) => "ol",
        TagEnd::List(false) => "ul",
        TagEnd::Paragraph => "p",
        TagEnd::Strong => "strong",
        TagEnd::Table => "table",
        TagEnd::TableCell => "td",
        TagEnd::TableRow => "tr",
        TagEnd::DefinitionList => "dl",
        TagEnd::DefinitionListTitle => "dt",
        TagEnd::DefinitionListDefinition => "dd",
        tag => panic!("Not a simple tag: {:?}", tag),
    }
}
