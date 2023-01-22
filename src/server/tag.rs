use super::templates::{self, RenderRucte};
use super::{
    goh, response, wrap, App, MyLang, Result, SlugAndLang, ViewError,
};
use crate::models::{Tag, Teaser};
use crate::schema::post_tags::dsl as pt;
use crate::schema::tags::dsl as t;
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use i18n_embed_fl::fl;
use tracing::instrument;
use warp::filters::BoxedFilter;
use warp::path::{end, param};
use warp::reply::Response;
use warp::{Filter, Reply};

pub fn routes(s: BoxedFilter<(App,)>) -> BoxedFilter<(impl Reply,)> {
    let cloud = param().and(end()).and(goh()).and(s.clone()).then(tagcloud);
    let page = param().and(end()).and(goh()).and(s).then(tagpage);
    cloud.or(page).unify().map(wrap).boxed()
}

#[instrument]
async fn tagcloud(lang: MyLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let tags = t::tags
        .left_join(pt::post_tags.on(pt::tag_id.eq(t::id)))
        .group_by(t::tags::all_columns())
        .select((t::tags::all_columns(), count_star()))
        .order(count_star().desc())
        .load::<(Tag, i64)>(&mut db)
        .await?;
    let n = tags.len();
    let m = 6;
    let mut tags = tags
        .into_iter()
        .enumerate()
        .map(|(i, (tag, j))| (tag, j, ((n - i - 1) * m) / n))
        .collect::<Vec<_>>();
    let col = lang.collator()?;
    tags.sort_by(|(a, _, _), (b, _, _)| {
        col.strcoll_utf8(&a.name, &b.name).unwrap()
    });

    let fluent = lang.fluent()?;
    let other_langs = lang.other(|_, lang, name| {
        format!(
            "<a href='/tag/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
            lang=lang, name=name,
        )});

    Ok(response()
        .html(|o| templates::tags_html(o, &fluent, &tags, &other_langs))?)
}

#[instrument]
async fn tagpage(tag: SlugAndLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let lang = tag.lang;
    let langc = lang.clone();
    let tag = Tag::by_slug(&tag.slug, &mut db)
        .await?
        .ok_or(ViewError::NotFound)?;

    let tag_id = tag.id;
    let posts = Teaser::tagged(tag_id, lang.as_ref(), 50, &mut db).await?;

    let fluent = langc.fluent()?;
    let h1 = fl!(fluent, "posts-tagged", tag = tag.name);
    let other_langs = langc.other(|_, lang, name| {
        format!(
            "<a href='/tag/{tag}.{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
            tag=tag.slug, lang=lang, name=name,
        )});

    let feed = format!("{}/atom-{}-{}.xml", app.base, langc, tag.slug);
    Ok(response().html(|o| {
        templates::posts_html(
            o,
            &fluent,
            &h1,
            Some(&feed),
            &posts,
            &[],
            &other_langs,
        )
    })?)
}
