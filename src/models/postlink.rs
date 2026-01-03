use super::{MyLang, Slug, year_of_date};
use crate::schema::posts;
use diesel::helper_types::{AsSelect, Select};
use diesel::pg::Pg;
use diesel::prelude::*;

#[derive(Debug, Queryable, Selectable, Identifiable)]
#[diesel(table_name = posts)]
pub struct PostLink {
    pub id: i32,
    #[diesel(select_expression = year_of_date(posts::posted_at))]
    pub year: i16,
    pub slug: Slug,
    pub lang: MyLang,
    pub title: String,
}

impl PostLink {
    pub fn all() -> Select<posts::table, AsSelect<PostLink, Pg>> {
        posts::table.select(Self::as_select())
    }
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year, self.slug, self.lang)
    }
}
