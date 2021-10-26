use crate::schema::posts::dsl as p;
use diesel::helper_types::Select;
use diesel::prelude::*;
use diesel::sql_types::{Smallint, Timestamptz};
use i18n_embed_fl::fl;

sql_function!(fn year_of_date(arg: Timestamptz) -> Smallint);

pub type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Queryable)]
pub struct User {
    id: i32,
    username: String,
    realname: String,
}

#[derive(Debug, Queryable)]
pub struct PostLink {
    pub id: i32,
    pub year: i16,
    pub slug: String,
    pub lang: String,
    pub title: String,
}

impl PostLink {
    pub fn select() -> Select<
        p::posts,
        (
            p::id,
            year_of_date::year_of_date<p::posted_at>,
            p::slug,
            p::lang,
            p::title,
        ),
    > {
        p::posts.select((
            p::id,
            year_of_date(p::posted_at),
            p::slug,
            p::lang,
            p::title,
        ))
    }
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year, self.slug, self.lang)
    }
}

#[derive(Debug, Queryable)]
pub struct Post {
    pub id: i32,
    pub year: i16,
    pub slug: String,
    pub lang: String,
    pub title: String,
    pub posted_at: DateTime,
    pub updated_at: DateTime,
    pub content: String,
}

impl Post {
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year, self.slug, self.lang)
    }
    pub fn publine(&self) -> String {
        let lang = crate::server::language::load(&self.lang).unwrap();
        let line = fl!(lang, "posted-at", date = self.posted_at.to_string());

        if self.updated_at > self.posted_at {
            line + &fl!(
                lang,
                "updated-at",
                date = self.updated_at.to_string()
            )
        } else {
            line
        }
        // TODO: tags.
    }
}
