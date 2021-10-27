use super::templates::{self, RenderRucte};
use super::{
    goh, language, wrap, MyLang, Pool, Result, SlugAndLang, ViewError,
};
use crate::models::{year_of_date, Post, Tag};
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use diesel::prelude::*;
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::reply::Response;
use warp::{Filter, Reply};

pub fn routes(s: BoxedFilter<(Pool,)>) -> BoxedFilter<(impl Reply,)> {
    use warp::path::{end, param};

    let cloud = param()
        .and(end())
        .and(goh())
        .and(s.clone())
        .and_then(|lang, a| async move { wrap(tagcloud(lang, a).await) });
    let page = param()
        .and(end())
        .and(goh())
        .and(s)
        .and_then(|tag, a| async move { wrap(tagpage(tag, a).await) });
    cloud.or(page).unify().boxed()
}

async fn tagcloud(lang: MyLang, pool: Pool) -> Result<Response> {
    let db = pool.get().await?;
    let tags = db
        .interact(move |db| {
            use diesel::dsl::sql;
            use diesel::sql_types::Integer;
            let c = sql::<Integer>("cast(count(*) as integer)");
            t::tags
                .left_join(pt::post_tags.on(pt::tag_id.eq(t::id)))
                .select((t::tags::all_columns(), c.clone()))
                .group_by(t::tags::all_columns())
                .order(c.desc())
                .load::<(Tag, i32)>(db)
        })
        .await??;
    let n = tags.len();
    let m = 6;
    let mut tags = tags
        .into_iter()
        .enumerate()
        .map(|(i, (tag, j))| (tag, j, ((n - i - 1) * m) / n))
        .collect::<Vec<_>>();
    tags.sort_by(|(a, _, _), (b, _, _)| a.slug.cmp(&b.slug));
    let fluent = language::load(&lang.0)?;
    Ok(Builder::new()
        .html(|o| templates::tags(o, &fluent, &tags))
        .unwrap())
}

async fn tagpage(tag: SlugAndLang, pool: Pool) -> Result<Response> {
    let db = pool.get().await?;
    let lang = tag.lang;
    let fluent = language::load(&lang)?;
    let tag = db
        .interact(move |db| {
            t::tags
                .filter(t::slug.eq(tag.slug))
                .first::<Tag>(db)
                .optional()
        })
        .await??
        .ok_or(ViewError::NotFound)?;

    let tag_id = tag.id;
    let posts = db
        .interact(move |db| {
            use diesel::dsl::sql;
            use diesel::sql_types::Bool;
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
                    sql::<Bool>(&format!("bool_or(lang='{}') over (partition by year_of_date(posted_at), slug)", lang))
                ))
                .filter(p::id.eq_any(pt::post_tags.select(pt::post_id).filter(pt::tag_id.eq(tag_id))))
                .order(p::updated_at.desc())
                .load::<(Post, bool)>(db)
                .and_then(|data| data.into_iter()
                .filter_map(|(post, langq)| {
                    if post.lang == lang || !langq {
                        Some(post)
                    } else {
                        None
                    }
                })
                .map(|post| {
                    Tag::for_post(post.id, db).map(|tags| (post, tags))
                })
                .collect::<Result<Vec<_>, _>>())
        })
        .await??;

    Ok(Builder::new()
        .html(|o| templates::frontpage(o, &fluent, &posts, &[]))
        .unwrap())
}
