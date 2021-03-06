use super::{year_of_date, Post, Result, Slug};
use crate::schema::posts::dsl as p;
use diesel::pg::PgConnection;
use diesel::prelude::*;

#[derive(Debug, Queryable)]
pub struct FullPost {
    post: Post,
    pub front_image: Option<String>,
    pub description: String,
    pub use_leaflet: bool,
}

impl std::ops::Deref for FullPost {
    type Target = Post;
    fn deref(&self) -> &Post {
        &self.post
    }
}

impl FullPost {
    pub fn load(
        year: i16,
        slug: &Slug,
        lang: &str,
        db: &PgConnection,
    ) -> Result<Option<FullPost>> {
        p::posts
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::content,
                ),
                p::front_image,
                p::description,
                p::use_leaflet,
            ))
            .filter(year_of_date(p::posted_at).eq(&year))
            .filter(p::slug.eq(slug.as_ref()))
            .filter(p::lang.eq(lang))
            .first::<FullPost>(db)
            .optional()
    }
}
