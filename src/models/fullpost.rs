use super::{
    LangLink, MyLang, Post, PostTag, Result, Slug, Tag, year_of_date,
};
use crate::dbopt::Connection;
use crate::schema::posts::dsl as p;
use diesel::associations::HasTable as _;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Debug, Queryable)]
pub struct FullPost {
    post: Post,
    pub tags: Vec<Tag>,
    pub front_image: Option<String>,
    pub description: String,
    pub use_leaflet: bool,
    pub other_langs: Vec<LangLink>,
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
        let Some((post, front_image, description, use_leaflet)) = p::posts
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
            .filter(p::lang.eq(&lang))
            .first::<(Post, Option<String>, String, bool)>(db)
            .await
            .optional()?
        else {
            return Ok(None);
        };

        let other_langs = p::posts
            .select((p::lang, p::title))
            .filter(year_of_date(p::posted_at).eq(&year))
            .filter(p::slug.eq(slug.as_ref()))
            .filter(p::lang.ne(&lang))
            .load::<(MyLang, String)>(db)
            .await?
            .into_iter()
            .map(|(lang, title)| LangLink::post(year, slug, lang, title))
            .collect::<Vec<_>>();

        let tags = PostTag::belonging_to(&post)
            .inner_join(Tag::table())
            .select(Tag::as_select())
            .load(db)
            .await?;

        Ok(Some(FullPost {
            post,
            tags,
            front_image,
            description,
            use_leaflet,
            other_langs,
        }))
    }
    pub fn publine(&self) -> String {
        self.post.publine(&self.tags)
    }
}
