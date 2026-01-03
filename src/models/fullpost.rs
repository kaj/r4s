use super::{Post, Result, Slug, year_of_date};
use crate::dbopt::Connection;
use crate::schema::posts::dsl as p;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

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
    pub async fn load(
        year: i16,
        slug: &Slug,
        lang: &str,
        db: &mut Connection,
    ) -> Result<Option<FullPost>> {
        p::posts
            .select((
                (
                    p::id,
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
            .await
            .optional()
    }
}
