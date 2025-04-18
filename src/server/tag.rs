use std::collections::BTreeMap;

use super::templates::{self, RenderRucte};
use super::{goh, response, App, MyLang, Result, SlugAndLang, ViewError};
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
    cloud.or(page).unify().boxed()
}

#[instrument]
async fn tagcloud(lang: MyLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let tags = t::tags
        .left_join(pt::post_tags)
        .group_by(t::tags::all_columns())
        .select((Tag::as_select(), count_star()))
        .order(t::name)
        .load::<(Tag, i64)>(&mut db)
        .await?;

    let m = 6; // Matches number of .wN classes in css.

    let mut counts = BTreeMap::<i64, usize>::new();
    for (_, n) in &tags {
        *counts.entry(*n).or_default() += 1;
    }
    //eprintln!("{counts:?}");
    let bin = (counts.len() + m) / (m - 1);
    for (i, (_key, value)) in counts.iter_mut().enumerate() {
        *value = i.div_ceil(bin);
    }
    //eprintln!("{counts:?}");

    let tags = tags
        .into_iter()
        .map(|(tag, j)| (tag, j, counts.get(&j).cloned().unwrap_or_default()))
        .collect::<Vec<_>>();

    let fluent = lang.fluent()?;
    let other_langs = lang.other(|_, lang, name| {
        format!(
            "<a href='/tag/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
        )});

    Ok(response()
        .html(|o| templates::tags_html(o, &fluent, &tags, &other_langs))?)
}

#[instrument]
async fn tagpage(tag: SlugAndLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let lang = tag.lang;
    let tag = Tag::by_slug(&tag.slug, &mut db)
        .await?
        .ok_or(ViewError::NotFound)?;

    let posts = Teaser::tagged(tag.id, lang.as_ref(), 50, &mut db).await?;

    let fluent = lang.fluent()?;
    let h1 = fl!(fluent, "posts-tagged", tag = tag.name);
    let other_langs = lang.other(|_, lang, name| {
        format!(
            "<a href='/tag/{tag}.{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
            tag=tag.slug,
        )});

    let feed = format!("{}/atom-{}-{}.xml", app.base, lang, tag.slug);
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
