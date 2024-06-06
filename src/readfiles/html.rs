//! How to serialize parsed markdown into my kind of html
use super::codeblocks::{BlockHandler, DynBlock};
use super::{FaRef, Loader};
use anyhow::{bail, Context, Result};
use lazy_regex::regex_captures;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use pulldown_cmark_escape::{escape_href, escape_html};
use tracing::warn;
use std::fmt::Write;

pub(super) fn collect<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
    loader: &mut Loader,
    year: i16,
    lang: &str,
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
                    year,
                    lang,
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
                // TODO: Respect id.
                let _ = remove_end(&mut result, "<p>")
                    || remove_end(&mut result, "<p><!--no-p-->")
                    || remove_end(&mut result, "<p><!--no-p-->\n");
                let mut inner = String::new();
                for tag in &mut data {
                    match tag {
                        Event::End(TagEnd::Image) => break,
                        Event::Text(text) => inner.push_str(&text),
                        Event::SoftBreak => inner.push(' '),
                        _ => inner.push_str(&format!("\n{:?}", tag)),
                    }
                }
                let (_all, imgref, _, classes, attrs, caption) = regex_captures!(
                    r#"^([A-Za-z0-9/._-]*)\s*(\{([\s\w]*)((?:\s[\w-]*="[^"]+")*)\})?\s*([^{]*)$"#m,
                    &dest_url,
                )
                .with_context(|| {
                    format!("Bad image ref: {:?}", dest_url.as_ref())
                })?;

                if classes.split_ascii_whitespace().any(|w| w == "gallery")
                    && !remove_end(&mut result, "</div><!--gallery-->\n")
                {
                    result.push_str("<div class='gallery'>");
                }

                if imgref == "cover" {
                    let url = inner.parse::<FaRef>().unwrap().cover();
                    writeln!(
                        &mut result,
                        "<figure class='fa-cover {}'>\
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
                    let imgtag = if classes
                        .split_ascii_whitespace()
                        .any(|w| w == "scaled")
                    {
                        imgdata.markup_large(alt)
                    } else {
                        imgdata.markup(alt)
                    };
                    let class2 = if imgdata.is_portrait() {
                        " portrait"
                    } else {
                        ""
                    };
                    writeln!(
                        &mut result,
                        "<figure class='{}{}'{}>{}\
                         <figcaption>{} {}</figcaption></figure>",
                        classes,
                        class2,
                        attrs,
                        imgtag,
                        caption,
                        title,
                    )
                    .unwrap();
                }
                if classes.split_ascii_whitespace().any(|w| w == "gallery") {
                    result.push_str("</div><!--gallery-->\n");
                }
                result.push_str("<p><!--no-p-->");
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
        tag => panic!("Not a simple tag: {:?}", tag),
    }
}
fn tag_name_e(tag: &TagEnd) -> &'static str {
    match tag {
        TagEnd::BlockQuote => "blockquote",
        TagEnd::Emphasis => "em",
        TagEnd::Item => "li",
        TagEnd::Link => "a",
        TagEnd::List(true) => "ul",
        TagEnd::List(false) => "ol",
        TagEnd::Paragraph => "p",
        TagEnd::Strong => "strong",
        TagEnd::Table => "table",
        TagEnd::TableCell => "td",
        TagEnd::TableRow => "tr",
        tag => panic!("Not a simple tag: {:?}", tag),
    }
}
