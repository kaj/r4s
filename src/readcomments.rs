//! Read comments from a json dump.  This is kind of a one-time operation.
use crate::dbopt::DbOpt;
use crate::models::{safe_md2html, year_of_date};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use anyhow::{anyhow, Context, Result};
use diesel::prelude::*;
use serde::{self, Deserialize, Deserializer};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    path: PathBuf,
}

impl Args {
    pub async fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        let file = File::open(&self.path)
            .with_context(|| format!("Failed to read {:?}", self.path))?;
        for comment in serde_json::from_reader::<_, Vec<Dumped>>(file)? {
            let post = &comment.on;
            let post: i32 = p::posts
                .select(p::id)
                .filter(year_of_date(p::posted_at).eq(&post.year))
                .filter(p::slug.eq(&post.slug))
                .filter(p::lang.eq(&post.lang))
                .first(&db)?;

            diesel::insert_into(c::comments)
                .values((
                    c::post_id.eq(post),
                    c::content.eq(comment.html()),
                    c::posted_at.eq(localdate(&comment.date)?),
                    c::name.eq(&comment.by_name),
                    c::email.eq(&comment.by_email),
                    comment.by_url.as_ref().map(|u| c::url.eq(u)),
                    c::raw_md.eq(&comment.comment),
                    c::is_public.eq(true),
                ))
                .execute(&db)?;
        }
        Ok(())
    }
}

fn localdate(date: &str) -> Result<DateTime> {
    use chrono::{Local, TimeZone};
    date.parse::<chrono::NaiveDateTime>()
        .with_context(|| format!("Bad pubdate: {:?}", date))
        .and_then(|d| {
            Ok(Local
                .from_local_datetime(&d)
                .earliest()
                .ok_or_else(|| anyhow!("Impossible local date"))?
                .into())
        })
}

#[derive(Debug, Deserialize)]
struct Dumped {
    by_name: String,
    by_email: String,
    by_url: Option<String>,
    comment: String,
    date: String,
    #[serde(deserialize_with = "deserialize_post")]
    on: PostRef,
}

impl Dumped {
    fn html(&self) -> String {
        safe_md2html(&self.comment)
    }
}

#[derive(Debug)]
struct PostRef {
    year: i16,
    slug: String,
    lang: String,
}
use lazy_regex::regex_captures;

impl FromStr for PostRef {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        regex_captures!(r"^/([0-9]{4})/([a-z-]+)\.([a-z]+)$", s)
            .ok_or_else(|| anyhow!("Bad post"))
            .and_then(|(_, year, slug, lang)| {
                Ok(PostRef {
                    year: year.parse()?,
                    slug: slug.into(),
                    lang: lang.into(),
                })
            })
    }
}

fn deserialize_post<'de, D>(deserializer: D) -> Result<PostRef, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    PostRef::from_str(&s).map_err(serde::de::Error::custom)
}

type DateTime = chrono::DateTime<chrono::FixedOffset>;
