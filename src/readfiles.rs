use crate::dbopt::DbOpt;
use crate::models::year_of_date;
use crate::schema::posts::dsl as p;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local};
use diesel::prelude::*;
use pulldown_cmark::{html, Event, Options, Parser, Tag};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::prelude::*;
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

fn items_until<'a>(all: &mut Vec<Event<'a>>, delimiter: &Event) -> Option<Vec<Event<'a>>> {
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

fn read_file(path: &Path, db: &PgConnection) -> Result<()> {
    let (slug, lang) = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Bad name"))?
        .split_once('.')
        .ok_or_else(|| anyhow!("No language in file name"))?;
    let mut file = File::open(path)?;
    let mut contents_md = String::new();
    file.read_to_string(&mut contents_md)?;
    let mut items = md_parser(&contents_md).collect::<Vec<_>>();

    let metadata = items_until(&mut items, &Event::Start(Tag::Heading(1)))
        .ok_or_else(|| anyhow!("No start of h1"))?;
    let metadata = dbg!(collect_metadata(metadata));

    let title = items_until(&mut items, &Event::End(Tag::Heading(1)))
        .ok_or_else(|| anyhow!("No start of h1"))?;
    let title = collect_html(title);

    let body = collect_html(items);

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
        if old_md == contents_md {
            println!("Post #{} exists", id);
        } else {
            println!("Post #{} exists, but should be updated", id);
            diesel::update(p::posts)
                .filter(p::id.eq(id))
                .set((
                    p::title.eq(&title),
                    p::content.eq(&body),
                    p::orig_md.eq(&contents_md),
                ))
                .execute(db)
                .context(format!("Update #{}", id))?;
        }
    } else {
        println!("new post {} at {:?}", slug, pubdate);
        diesel::insert_into(p::posts)
            .values((
                pubdate.map(|date| p::posted_at.eq(date)),
                pubdate.map(|date| p::updated_at.eq(date)),
                p::slug.eq(slug),
                p::lang.eq(lang),
                p::title.eq(&title),
                p::content.eq(&body),
                p::orig_md.eq(&contents_md),
            ))
            .execute(db)?;
    }
    Ok(())
}

fn md_parser(markdown: &str) -> Parser {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    Parser::new_ext(markdown, options)
}

fn collect_html<'a>(data: impl IntoIterator<Item = Event<'a>>) -> String {
    let mut result = String::new();
    html::push_html(&mut result, data.into_iter());
    result
}

fn collect_metadata<'a>(data: impl IntoIterator<Item = Event<'a>>) -> BTreeMap<String, String> {
    data.into_iter()
        .flat_map(|e| match e {
            Event::Text(t) => t
                .split_once(':')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())),
            _ => None,
        })
        .collect()
}
