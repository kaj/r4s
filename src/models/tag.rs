use super::{Result, Slug};
use crate::schema::post_tags::dsl as pt;
use crate::schema::tags::dsl as t;
use diesel::pg::PgConnection;
use diesel::prelude::*;

#[derive(Debug, Queryable)]
pub struct Tag {
    pub id: i32,
    pub slug: Slug,
    pub name: String,
}

impl Tag {
    pub fn by_slug(slug: &Slug, db: &PgConnection) -> Result<Option<Tag>> {
        t::tags
            .filter(t::slug.eq(slug.as_ref()))
            .first::<Tag>(db)
            .optional()
    }
    pub fn for_post(post_id: i32, db: &PgConnection) -> Result<Vec<Tag>> {
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
