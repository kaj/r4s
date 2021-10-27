use crate::dbopt::DbOpt;
use crate::imgcli::ImageInfo;
use crate::models::year_of_date;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local};
use diesel::prelude::*;
use lazy_regex::regex_captures;
use pulldown_cmark::{BrokenLink, Event, Options, Parser, Tag};
use slug::slugify;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

type DateTime = chrono::DateTime<chrono::FixedOffset>;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// The paths to read content from.
    #[structopt(parse(from_os_str))]
    files: Vec<PathBuf>,

    /// Update content even if it has not changed.
    ///
    /// Mainly usefull while developing r4s itself.
    #[structopt(long)]
    force: bool,
}

impl Args {
    pub async fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        for path in self.files {
            read_file(&path, self.force, &db)
                .await
                .context(format!("Reading {:?}", path))?;
        }
        Ok(())
    }
}

async fn read_file(
    path: &Path,
    force: bool,
    db: &PgConnection,
) -> Result<()> {
    let (slug, lang) = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| anyhow!("Bad name"))?
        .split_once('.')
        .ok_or_else(|| anyhow!("No language in file name"))?;
    let contents = read_to_string(path)?;
    let (metadata, contents_md) = extract_metadata(&contents);

    let pubdate = metadata
        .get("pubdate")
        .map(|v| v.parse::<DateTime>().unwrap());
    let year = pubdate.unwrap_or_else(|| Local::now().into()).year() as i16;
    if let Some((id, old_md)) = p::posts
        .select((p::id, p::orig_md))
        .filter(year_of_date(p::posted_at).eq(&year))
        .filter(p::slug.eq(slug))
        .filter(p::lang.eq(lang))
        .first::<(i32, String)>(db)
        .optional()?
    {
        if old_md == contents && !force {
            println!("Post #{} /{}/{}.{} exists", id, year, slug, lang);
        } else {
            println!(
                "Post #{} /{}/{}.{} exists, but should be updated.\n   {:?}",
                id, year, slug, lang, metadata
            );
            let (title, body) = md_to_html(contents_md).await?;
            diesel::update(p::posts)
                .filter(p::id.eq(id))
                .set((
                    p::title.eq(&title),
                    p::content.eq(&body),
                    p::orig_md.eq(&contents),
                ))
                .execute(db)
                .context(format!("Update #{}", id))?;
            if let Some(tags) = metadata.get("tags") {
                tag_post(id, tags, db)?;
            }
        }
    } else {
        println!("New post /{}/{}.{}\n   {:?}", year, slug, lang, metadata);
        let (title, body) = md_to_html(contents_md).await?;
        let post_id = diesel::insert_into(p::posts)
            .values((
                pubdate.map(|date| p::posted_at.eq(date)),
                pubdate.map(|date| p::updated_at.eq(date)),
                p::slug.eq(slug),
                p::lang.eq(lang),
                p::title.eq(&title),
                p::content.eq(&body),
                p::orig_md.eq(&contents),
            ))
            .returning(p::id)
            .get_result::<i32>(db)
            .context("Insert post")?;
        if let Some(tags) = metadata.get("tags") {
            tag_post(post_id, tags, db)?;
        }
    }
    Ok(())
}

fn tag_post(post_id: i32, tags: &str, db: &PgConnection) -> Result<()> {
    use crate::models::Tag;
    for tag in tags.split(',') {
        let tag = tag.trim();
        let tag = t::tags
            .filter(t::name.ilike(&tag))
            .first::<Tag>(db)
            .or_else(|_| {
                diesel::insert_into(t::tags)
                    .values((t::name.eq(&tag), t::slug.eq(&slugify(&tag))))
                    .get_result::<Tag>(db)
            })
            .context("Find or create tag")?;
        diesel::insert_into(pt::post_tags)
            .values((pt::post_id.eq(post_id), pt::tag_id.eq(tag.id)))
            .execute(db)
            .context("tag post")?;
    }
    Ok(())
}

/// Convert my flavour of markdown to my preferred html.
///
/// Returns the title and body html markup separately.
async fn md_to_html(markdown: &str) -> Result<(String, String)> {
    let mut fixlink = |broken_link: BrokenLink| {
        Some(if let Some(url) = fa_link(broken_link.reference) {
            (url.into(), String::new().into())
        } else {
            link_ext(&broken_link, markdown)
                .map(|(url, title)| (url.into(), title.into()))
                .unwrap_or((
                    broken_link.reference.to_owned().into(),
                    String::new().into(),
                ))
        })
    };
    let mut items = Parser::new_with_broken_link_callback(
        markdown,
        Options::all(),
        Some(&mut fixlink),
    )
    .collect::<Vec<_>>();

    let prefix = items_until(&mut items, &Event::Start(Tag::Heading(1)))
        .ok_or_else(|| anyhow!("No start of h1"))?;
    if !prefix.is_empty() {
        return Err(anyhow!("Expected empty prefix, got {:?}", prefix));
    }

    let title = items_until(&mut items, &Event::End(Tag::Heading(1)))
        .ok_or_else(|| anyhow!("No end of h1"))?;
    let title = collect_html(title).await;

    let body = collect_html(items.into_iter()).await;

    Ok((dbg!(title), body))
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

/// Check if `s` is a phantom issue reference.
///
/// Strings like `"Fa 1/1950"` or `"Fa 2-3 2019"` gets an index url.
fn fa_link(s: &str) -> Option<String> {
    let (_, _, i, _, y, _) = regex_captures!(
        r"\b[Ff]a (?P<ii>(?P<i>[1-9]\d?)(-[1-9]\d?)?)[ /](?P<y>(19|20)\d{2})\b",
        s,
    )?;
    Some(format!("https://fantomenindex.krats.se/{}/{}", y, i))
}
#[test]
fn fa_link_a() {
    assert_eq!(fa_link("Hello"), None)
}
#[test]
fn fa_link_b() {
    assert_eq!(
        fa_link("Fa 17/1984").as_deref(),
        Some("https://fantomenindex.krats.se/1984/17")
    )
}
#[test]
fn fa_link_c() {
    assert_eq!(
        fa_link("Fa 1-2/2021").as_deref(),
        Some("https://fantomenindex.krats.se/2021/1")
    )
}

fn link_ext(link: &BrokenLink, source: &str) -> Option<(String, String)> {
    let (_all, text, kind, _, _attrs) = regex_captures!(
        r"^\[(.*)\]\[(\w+)([,\s]+(.*))?\]$",
        &source[link.span.clone()],
    )?;
    match kind {
        "personname" => {
            let lang = "sv"; // FIXME
            Some((
                format!(
                    "https://{}.wikipedia.org/wiki/{}",
                    lang,
                    text.replace(' ', "_")
                ),
                format!("Se {} på wikipedia", text),
            ))
        }
        "cargo" => {
            Some((format!("https://lib.rs/crates/{}", text), String::new()))
        }
        _ => None,
    }
}

async fn collect_html<'a>(
    data: impl IntoIterator<Item = Event<'a>>,
) -> String {
    let mut result = String::new();
    let mut data = data.into_iter();
    let mut section_level = 1;
    while let Some(event) = data.next() {
        match event {
            Event::Text(text) => result.push_str(text.as_ref()),
            Event::Start(Tag::Heading(level)) => {
                while section_level >= level {
                    result.push_str("</section>");
                    section_level -= 1;
                }
                result.push('\n');
                while section_level < level {
                    result.push_str("<section>");
                    section_level += 1;
                }
                result.push_str(&format!("<h{}>", level));
            }
            Event::End(Tag::Heading(level)) => {
                result.push_str(&format!("</h{}>\n", level));
            }
            Event::Start(Tag::CodeBlock(blocktype)) => {
                result.push_str("<pre><code");
                result.push_str(&format!(" data-type='{:?}'", blocktype));
                result.push('>');
            }
            Event::End(Tag::CodeBlock(_blocktype)) => {
                result.push_str("</code></pre>\n");
            }
            Event::Start(Tag::Image(imgtype, imgref, title)) => {
                if result.ends_with("<p>") {
                    result.truncate(result.len() - 3);
                }
                let mut inner = String::new();
                for tag in &mut data {
                    match tag {
                        Event::End(Tag::Image(..)) => break,
                        Event::Text(text) => inner.push_str(&text),
                        Event::SoftBreak => inner.push(' '),
                        _ => inner.push_str(&format!("\n{:?}", tag)),
                    }
                }
                let (_all, imgref, _, classes, caption) = regex_captures!(
                    r"^([A-Za-z0-9/._-]*)\s*(\{([^}]*)\})?\s*(.*)$",
                    &imgref,
                )
                .unwrap_or_else(|| {
                    panic!("Bad image ref: {:?}", imgref.as_ref())
                });
                let imgdata =
                    ImageInfo::fetch(imgref).await.expect("Image api");
                let alt = inner.trim();
                let imgtag = if classes == "scaled" {
                    imgdata.markup_large(alt)
                } else {
                    imgdata.markup(alt)
                };
                write!(
                    &mut result,
                    "<figure class='{}' data-type='{:?}'>{}\
                     <figcaption>{} {}</figcaption></figure>\n<p><!--no-p-->",
                    classes, imgtype, imgtag, caption, title,
                )
                .unwrap();
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
                    Tag::Link(linktype, a, b) => {
                        if !a.is_empty() {
                            result.push_str(" href='");
                            result.push_str(a.as_ref());
                            result.push('\'');
                        }
                        if !b.is_empty() {
                            result.push_str(" title='");
                            result.push_str(b.as_ref());
                            result.push('\'');
                        }
                        result.push_str(&format!(
                            " data-type='{:?}'",
                            linktype
                        ));
                    }
                    t => result.push_str(&format!("><!-- {:?} --", t)),
                }
                // TODO: Link attributes!
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
            Event::SoftBreak => result.push('\n'),
            Event::Html(code) => result.push_str(&code),
            Event::Code(code) => {
                result.push_str("<code>");
                pulldown_cmark::escape::escape_html(&mut result, &code)
                    .unwrap();
                result.push_str("</code>");
            }
            Event::HardBreak => {
                result.push_str("<br/>\n");
            }
            e => panic!("Unhandled: {:?}", e),
        }
    }
    for _ in 2..=section_level {
        result.push_str("</section>");
    }
    result
}

fn tag_name(tag: &Tag) -> &'static str {
    match tag {
        Tag::Paragraph => "p",
        Tag::Emphasis => "em",
        //Tag::Image(..) => "a", // no, not really!
        Tag::Link(..) => "a",
        Tag::Table(..) => "table",
        Tag::TableRow => "tr",
        Tag::TableCell => "td",
        Tag::List(Some(_)) => "ol",
        Tag::List(None) => "ul",
        Tag::Item => "li",
        tag => panic!("Not a simple tag: {:?}", tag),
    }
}

fn extract_metadata(src: &str) -> (BTreeMap<&str, &str>, &str) {
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
    (meta, src)
}
