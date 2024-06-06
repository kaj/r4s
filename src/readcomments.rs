//! Read comments from a json dump.  This is kind of a one-time operation.
use crate::dbopt::DbOpt;
use crate::models::{safe_md2html, year_of_date, DateTime};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use crate::schema::{comments, posts};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use lazy_regex::regex_captures;
use serde::{self, Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// Path name of json file to read comments from.
    path: PathBuf,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let mut db = self.db.get_db()?;
        let file = File::open(&self.path)
            .with_context(|| format!("Failed to read {:?}", self.path))?;
        for comment in serde_json::from_reader::<_, Vec<Dumped>>(file)? {
            let post = &comment.on;
            let post: i32 = p::posts
                .select(p::id)
                .filter(year_of_date(p::posted_at).eq(&post.year))
                .filter(p::slug.eq(&post.slug))
                .filter(p::lang.eq(&post.lang))
                .first(&mut db)?;

            diesel::insert_into(c::comments)
                .values((
                    c::post_id.eq(post),
                    c::content.eq(comment.html()),
                    c::posted_at.eq(&comment.date.raw()),
                    c::name.eq(&comment.by_name),
                    c::email.eq(&comment.by_email),
                    comment.by_url.as_ref().map(|u| c::url.eq(u)),
                    c::from_host.eq(comment.by_ip),
                    c::raw_md.eq(&comment.comment),
                    c::is_public.eq(true),
                ))
                .execute(&mut db)?;
        }
        Ok(())
    }
}

#[derive(Parser)]
pub struct DumpArgs {
    #[clap(flatten)]
    db: DbOpt,

    /// Path name to write comments json data to.
    path: PathBuf,
}

impl DumpArgs {
    pub fn run(self) -> Result<()> {
        let comments = comments::table
            .inner_join(posts::table)
            .select(Dumped::as_select())
            .load(&mut self.db.get_db()?)?;
        std::fs::write(&self.path, serde_json::to_string_pretty(&comments)?)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, Queryable, Selectable)]
#[diesel(table_name = comments)]
struct Dumped {
    #[diesel(column_name = name)]
    by_name: String,
    #[diesel(column_name = email)]
    by_email: String,
    #[diesel(column_name = url)]
    by_url: Option<String>,
    #[diesel(column_name = from_host)]
    by_ip: IpNetwork,
    #[diesel(column_name = raw_md)]
    comment: String,
    #[diesel(column_name = posted_at)]
    #[serde(with = "serde_date")]
    date: DateTime,
    #[diesel(embed, column_name = post_id)]
    #[serde(with = "serde_post")]
    on: PostRef,
}

impl Dumped {
    fn html(&self) -> String {
        safe_md2html(&self.comment)
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = posts)]
struct PostRef {
    #[diesel(select_expression = year_of_date(p::posted_at),
             select_expression_type=year_of_date::year_of_date<p::posted_at>)]
    year: i16,
    slug: String,
    lang: String,
}

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

mod serde_post {
    use super::PostRef;
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(p: &PostRef, dest: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        format!("/{}/{}.{}", p.year, p.slug, p.lang).serialize(dest)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PostRef, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

mod serde_date {
    use super::DateTime;
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(d: &DateTime, dest: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        d.raw().format("%Y-%m-%d %H:%M:%S%Z").to_string().serialize(dest)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
