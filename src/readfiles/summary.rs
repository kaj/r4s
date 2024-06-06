//! How to serialize parsed markdown into my kind of html
use anyhow::{bail, Result};
use lazy_regex::regex_replace_all;
use pulldown_cmark::{Event, Tag, TagEnd};
use pulldown_cmark_escape::escape_html;
use tracing::warn;

pub fn collect<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
) -> Result<String> {
    let mut result = String::new();
    let mut data = data.into_iter();
    while let Some(event) = data.next() {
        match event {
            Event::Text(text) => {
                escape_html(&mut result, &text)?;
            }
            Event::Start(Tag::Heading { .. }) => (),
            Event::End(TagEnd::Heading(_)) => {
                result.push_str(": ");
            }
            Event::Start(Tag::CodeBlock(_blocktype)) => {
                for event in &mut data {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(code) => escape_html(&mut result, &code)?,
                        x => bail!("Unexpeted in code: {:?}", x),
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                unreachable!();
            }
            Event::Start(Tag::Image { .. }) => {
                for event in &mut data {
                    if let Event::End(TagEnd::Image) = event {
                        break;
                    }
                }
            }
            Event::TaskListMarker(done) => {
                result.push(if done { '☑' } else { '☐' });
            }
            Event::Start(tag) => match tag {
                Tag::Paragraph
                | Tag::TableHead
                | Tag::TableCell
                | Tag::TableRow => {
                    result.push(' ');
                }
                Tag::Item => result.push_str(" * "),
                _ => (),
            },
            Event::End(tag) => match tag {
                TagEnd::Item
                | TagEnd::Paragraph
                | TagEnd::TableHead
                | TagEnd::TableCell
                | TagEnd::TableRow => {
                    result.push(' ');
                }
                _ => (),
            },
            Event::Rule => result.push_str(" -- "),
            Event::SoftBreak => result.push(' '),
            Event::Html(_code) => result.push(' '),
            Event::Code(code) => {
                escape_html(&mut result, &code)?;
            }
            Event::HardBreak => {
                result.push(' ');
            }
            Event::InlineHtml(code) => {
                warn!("Found raw html: {code:?}.");
                result.push_str(&code)
            }
            e => bail!("Unhandled: {:?}", e),
        }
    }
    Ok(regex_replace_all!(r"\s+", result.trim(), |_| " ").to_string())
}
