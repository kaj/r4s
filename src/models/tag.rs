use super::{Post, Result, Slug};
use crate::dbopt::Connection;
use crate::schema::{post_tags, tags};
use diesel::associations::HasTable;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Identifiable, Debug, Selectable, Queryable, PartialEq)]
pub struct Tag {
    pub id: i32,
    pub slug: Slug,
    pub name: String,
}

#[derive(Identifiable, Selectable, Queryable, Associations, Debug)]
#[diesel(belongs_to(Tag))]
#[diesel(belongs_to(Post))]
#[diesel(primary_key(post_id, tag_id))]
pub struct PostTag {
    pub post_id: i32,
    pub tag_id: i32,
}

impl Tag {
    pub async fn by_slug(
        slug: &Slug,
        db: &mut Connection,
    ) -> Result<Option<Tag>> {
        Tag::table()
            .filter(tags::slug.eq(slug.as_ref()))
            .first::<Tag>(db)
            .await
            .optional()
    }
}
