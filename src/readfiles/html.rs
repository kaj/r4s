//! How to serialize parsed markdown into my kind of html
use super::codeblocks::{BlockHandler, DynBlock};
use super::FaRef;
use super::ImgClient;
use anyhow::{bail, Context, Result};
use lazy_regex::regex_captures;
use pulldown_cmark::escape::{escape_href, escape_html};
use pulldown_cmark::{CodeBlockKind, Event, Tag};
use std::fmt::Write;

pub fn collect<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
    img_client: &mut ImgClient,
) -> Result<String> {
    let mut result = String::new();
    let mut data = data.into_iter();
    let mut section_level = 1;
    while let Some(event) = data.next() {
        match event {
            Event::Text(text) => {
                escape_html(&mut result, &text)?;
            }
            Event::Start(Tag::Heading(level, id, classes)) => {
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
                    escape_html(&mut result, id)?;
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
            Event::End(Tag::Heading(level, _, _)) => {
                if !remove_end(&mut result, &format!("<{}>", level)) {
                    result.push_str(&format!("</{}>\n", level));
                }
            }
            Event::Start(Tag::CodeBlock(blocktype)) => {
                let lang = match blocktype {
                    CodeBlockKind::Fenced(ref f) => {
                        (!f.is_empty()).then(|| f.as_ref())
                    }
                    CodeBlockKind::Indented => None,
                };
                let mut handler = DynBlock::for_kind(&mut result, lang)?;
                for event in &mut data {
                    match event {
                        Event::End(Tag::CodeBlock(_blocktype)) => break,
                        Event::Text(code) => handler.push(&code)?,
                        x => bail!("Unexpeted in code: {:?}", x),
                    }
                }
                handler.end();
            }
            Event::End(Tag::CodeBlock(_blocktype)) => {
                unreachable!();
            }
            Event::Start(Tag::Image(imgtype, imgref, title)) => {
                let _ = remove_end(&mut result, "<p>")
                    || remove_end(&mut result, "<p><!--no-p-->")
                    || remove_end(&mut result, "<p><!--no-p-->\n");
                let mut inner = String::new();
                for tag in &mut data {
                    match tag {
                        Event::End(Tag::Image(..)) => break,
                        Event::Text(text) => inner.push_str(&text),
                        Event::SoftBreak => inner.push(' '),
                        _ => inner.push_str(&format!("\n{:?}", tag)),
                    }
                }
                let (_all, imgref, _, classes, attrs, caption) = regex_captures!(
                    r#"^([A-Za-z0-9/._-]*)\s*(\{([\s\w]*)((?:\s[\w-]*="[^"]+")*)\})?\s*([^{]*)$"#m,
                    &imgref,
                )
                .with_context(|| {
                    format!("Bad image ref: {:?}", imgref.as_ref())
                })?;

                if classes.split_ascii_whitespace().any(|w| w == "gallery") {
                    if !remove_end(&mut result, "</div><!--gallery-->\n") {
                        result.push_str("<div class='gallery'>");
                    }
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
                    let imgdata = img_client.fetch(imgref)?;
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
                        "<figure class='{}{}'{} data-type='{:?}'>{}\
                         <figcaption>{} {}</figcaption></figure>",
                        classes,
                        class2,
                        attrs,
                        imgtype,
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
            Event::End(Tag::Paragraph)
                if result.ends_with("<p><!--no-p-->") =>
            {
                result.truncate(result.len() - 14);
            }
            Event::Start(Tag::TableHead) => {
                result.push_str("<thead><tr>");
            }
            Event::End(Tag::TableHead) => {
                result.push_str("</tr></thead>\n");
            }
            Event::TaskListMarker(done) => {
                result.push_str("<input disabled type='checkbox'");
                if done {
                    result.push_str(" checked=''");
                }
                result.push_str("/>\n");
            }
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
                    Tag::Link(linktype, href, title) => {
                        if !href.is_empty() {
                            result.push_str(" href=\"");
                            escape_href(&mut result, &href)?;
                            result.push('"');
                        }
                        if !title.is_empty() {
                            result.push_str(" title=\"");
                            escape_html(&mut result, &title)?;
                            result.push('"');
                        }
                        result.push_str(&format!(
                            " data-type='{:?}'",
                            linktype
                        ));
                    }
                    t => result.push_str(&format!("><!-- {:?} --", t)),
                }
                result.push('>');
            }
            Event::End(tag) => {
                result.push_str("</");
                result.push_str(tag_name(&tag));
                result.push('>');
                if matches!(
                    tag,
                    Tag::Paragraph
                        | Tag::Table(..)
                        | Tag::Item
                        | Tag::List(_)
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
        Tag::BlockQuote => "blockquote",
        Tag::Emphasis => "em",
        Tag::Item => "li",
        Tag::Link(..) => "a",
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
