use crate::schema::comments::dsl as c;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use diesel::helper_types::Select;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::sql_types::{Smallint, Timestamptz, Varchar};
use i18n_embed_fl::fl;

sql_function! {
    fn year_of_date(arg: Timestamptz) -> Smallint;
}

sql_function! {
    fn has_lang(yearp: Smallint, slugp: Varchar, langp: Varchar) -> Bool;
}

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
    pub fn publine(&self, tags: &[Tag]) -> String {
        use std::fmt::Write;
        let lang = crate::server::language::load(&self.lang).unwrap();
        let mut line =
            fl!(lang, "posted-at", date = self.posted_at.to_string());

        if self.updated_at > self.posted_at {
            write!(
                &mut line,
                " {}",
                fl!(lang, "updated-at", date = self.updated_at.to_string())
            )
            .unwrap();
        }
        if let Some((first, rest)) = tags.split_first() {
            write!(
                line,
                " {} <a href='/tag/{slug}.{lang}'>{name}</a>",
                fl!(lang, "tagged"),
                slug = first.slug,
                name = first.name,
                lang = self.lang,
            )
            .unwrap();
            for tag in rest {
                write!(
                    line,
                    ", <a href='/tag/{slug}.{lang}'>{name}</a>",
                    slug = tag.slug,
                    name = tag.name,
                    lang = self.lang,
                )
                .unwrap();
            }
            line.push('.');
        }
        line
    }
}

#[derive(Debug, Queryable)]
pub struct Tag {
    pub id: i32,
    pub slug: String,
    pub name: String,
}

impl Tag {
    pub fn for_post(
        post_id: i32,
        db: &PgConnection,
    ) -> Result<Vec<Tag>, diesel::result::Error> {
        t::tags
            .filter(
                t::id.eq_any(
                    pt::post_tags
                        .select(pt::tag_id)
                        .filter(pt::post_id.eq(post_id)),
                ),
            )
            .load(db)
    }
}

#[derive(Debug, Queryable)]
pub struct Comment {
    pub posted_at: DateTime,
    pub content: String,
    pub name: String,
    pub email: String,
    pub url: Option<String>,
}

impl Comment {
    pub fn for_post(
        post_id: i32,
        db: &PgConnection,
    ) -> Result<Vec<Comment>, diesel::result::Error> {
        c::comments
            .select((c::posted_at, c::content, c::name, c::email, c::url))
            .filter(c::post_id.eq(post_id))
            .order_by(c::posted_at.asc())
            .load::<Comment>(db)
    }
}
