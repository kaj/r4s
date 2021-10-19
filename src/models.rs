use crate::schema::posts::dsl as p;
use diesel::helper_types::Select;
use diesel::prelude::*;
use diesel::sql_types::Timestamptz;

sql_function!(fn year_of_date(arg: Timestamptz) -> Smallint);

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
