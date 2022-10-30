use super::{Result, Slug};
use crate::dbopt::Connection;
use crate::schema::post_tags::dsl as pt;
use crate::schema::tags::dsl as t;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Debug, Queryable)]
pub struct Tag {
    pub id: i32,
    pub slug: Slug,
    pub name: String,
}

impl Tag {
    pub async fn by_slug(
        slug: &Slug,
        db: &mut Connection,
    ) -> Result<Option<Tag>> {
        t::tags
            .filter(t::slug.eq(slug.as_ref()))
            .first::<Tag>(db)
            .await
            .optional()
    }
    pub async fn for_post(
        post_id: i32,
        db: &mut Connection,
    ) -> Result<Vec<Tag>> {
        t::tags
            .filter(
                t::id.eq_any(
                    pt::post_tags
                        .select(pt::tag_id)
                        .filter(pt::post_id.eq(post_id)),
                ),
            )
            .load(db)
            .await
    }
}
