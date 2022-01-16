use super::{has_lang, year_of_date, Post, Result, Tag};
use crate::schema::comments::dsl as c;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use diesel::dsl::{not, sql};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use i18n_embed_fl::fl;

pub struct Teaser {
    post: Post,
    tags: Vec<Tag>,
    /// True if the full text of the post is more than this teaser.
    is_more: bool,
    n_comments: u32,
}

impl Teaser {
    pub fn recent(
        lang: &str,
        limit: u32,
        db: &PgConnection,
    ) -> Result<Vec<Self>> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(p::lang.eq(lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.desc())
            .limit(limit.into())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn for_year(
        year: i16,
        lang: &str,
        db: &PgConnection,
    ) -> Result<Vec<Teaser>> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(
                year_of_date(p::posted_at)
                    .eq(year)
                    .or(year_of_date(p::updated_at).eq(year)),
            )
            .filter(p::lang.eq(lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.asc())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn tagged(
        tag_id: i32,
        lang: &str,
        limit: u32,
        db: &PgConnection,
    ) -> Result<Vec<Teaser>> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(
                p::id.eq_any(
                    pt::post_tags
                        .select(pt::post_id)
                        .filter(pt::tag_id.eq(tag_id)),
                ),
            )
            .filter(p::lang.eq(lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.desc())
            .limit(limit.into())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn publine(&self) -> String {
        // TODO: Take the fluent as an argument instead?
        let lang = crate::server::language::load(&self.lang).unwrap();
        self.post.publine(&lang, &self.tags)
    }
    pub fn readmore(&self) -> String {
        // TODO: Take the fluent as an argument instead?
        let lang = crate::server::language::load(&self.lang).unwrap();
        match (self.is_more, self.n_comments > 0) {
            (true, true) => fl!(
                lang,
                "read-more-comments",
                title = self.title.as_str(),
                n = self.n_comments
            ),
            (true, false) => {
                fl!(lang, "read-more", title = self.title.as_str())
            }
            (false, true) => fl!(lang, "read-comments", n = self.n_comments),
            (false, false) => fl!(lang, "comment-first"),
        }
    }
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }
}

impl std::ops::Deref for Teaser {
    type Target = Post;
    fn deref(&self) -> &Post {
        &self.post
    }
}
