use crate::dbopt::DbOpt;
use crate::models::year_of_date;
use crate::schema::posts::dsl as p;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local};
use diesel::prelude::*;
use lazy_regex::regex_captures;
use pulldown_cmark::{html, BrokenLink, Event, Options, Parser, Tag};
use std::collections::BTreeMap;
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
}

impl Args {
    pub fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        for path in self.files {
            read_file(&path, &db).context(format!("Reading {:?}", path))?;
        }
        Ok(())
    }
}

fn read_file(path: &Path, db: &PgConnection) -> Result<()> {
    let (slug, lang) = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Bad name"))?
        .split_once('.')
        .ok_or_else(|| anyhow!("No language in file name"))?;
    let contents = read_to_string(path)?;
    let (metadata, contents_md) = extract_metadata(&contents);

    let pubdate = dbg!(metadata)
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
        if old_md == contents {
            println!("Post #{} exists", id);
        } else {
            println!("Post #{} exists, but should be updated", id);
            let (title, body) = md_to_html(contents_md)?;
            diesel::update(p::posts)
                .filter(p::id.eq(id))
                .set((
                    p::title.eq(&title),
                    p::content.eq(&body),
                    p::orig_md.eq(&contents),
                ))
                .execute(db)
                .context(format!("Update #{}", id))?;
        }
    } else {
        println!("new post {} at {:?}", slug, pubdate);
        let (title, body) = md_to_html(contents_md)?;
        diesel::insert_into(p::posts)
            .values((
                pubdate.map(|date| p::posted_at.eq(date)),
                pubdate.map(|date| p::updated_at.eq(date)),
                p::slug.eq(slug),
                p::lang.eq(lang),
                p::title.eq(&title),
                p::content.eq(&body),
                p::orig_md.eq(&contents),
            ))
            .execute(db)?;
    }
    Ok(())
}

/// Convert my flavour of markdown to my preferred html.
///
/// Returns the title and body html markup separately.
fn md_to_html(markdown: &str) -> Result<(String, String)> {
    let mut fixlink = |broken_link: BrokenLink| {
        // dbg!(&broken_link.link_type);
        if let Some(url) = fa_link(broken_link.reference) {
            Some((url.into(), String::new().into()))
        } else if let Some(url) = cargo_link(&broken_link, markdown) {
            Some((url.into(), String::new().into()))
        } else {
            Some((
                dbg!(broken_link.reference).to_owned().into(),
                String::new().into(),
            ))
        }
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
    let title = collect_html(title);

    let body = collect_html(Sectioned::new(items.into_iter()));

    Ok((dbg!(title), body))
}

fn items_until<'a>(
    all: &mut Vec<Event<'a>>,
    delimiter: &Event,
) -> Option<Vec<Event<'a>>> {
    let pos = all.iter().position(|e| e == delimiter);
    if let Some(pos) = pos {
        let mut prefix = all.split_off(pos + 1);
        std::mem::swap(all, &mut prefix);
        prefix.pop(); // get rid of the delimiter itself
        Some(prefix)
    } else {
        None
    }
}

struct Sectioned<Iter> {
    inner: Iter,
    state: SectionedState,
}

impl<Iter> Sectioned<Iter> {
    fn new(inner: Iter) -> Self {
        let state = SectionedState::In(1);
        Sectioned { inner, state }
    }
}

impl<'a, Iter: Iterator<Item = Event<'a>>> Iterator for Sectioned<Iter> {
    type Item = Event<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            SectionedState::In(level) => match self.inner.next() {
                Some(Event::Start(Tag::Heading(i))) => {
                    self.state = SectionedState::Start(i);
                    let mut sep = String::new();
                    for _ in i..=level {
                        sep.push_str("</section>");
                    }
                    sep.push_str("<section>");
                    Some(Event::Html(sep.into()))
                }
                Some(x) => Some(x),
                None => {
                    if level > 1 {
                        let mut sep = String::new();
                        for _ in 2..=level {
                            sep.push_str("</section>");
                        }
                        self.state = SectionedState::In(1);
                        Some(Event::Html(sep.into()))
                    } else {
                        None
                    }
                }
            },
            SectionedState::Start(i) => {
                self.state = SectionedState::In(i);
                Some(Event::Start(Tag::Heading(i)))
            }
        }
    }
}

enum SectionedState {
    In(u32), // Normal state.  The number is the containing header level, defalt 1.
    Start(u32), // At the start of heading of specified level
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

/// Check if link is a `[somepkg][cargo]` link.
///
/// If it is, make a link to `lib.rs`.
fn cargo_link(link: &BrokenLink, contents_md: &str) -> Option<String> {
    if link.reference == "cargo" {
        let pkg = &contents_md[link.span.clone()]
            .strip_prefix('[')?
            .strip_suffix("][cargo]")?;
        Some(format!("https://lib.rs/crates/{}", pkg))
    } else {
        None
    }
}

fn collect_html<'a>(data: impl IntoIterator<Item = Event<'a>>) -> String {
    let mut result = String::new();
    html::push_html(&mut result, data.into_iter());
    result
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
        src = reminder
    }
    (meta, src)
}
