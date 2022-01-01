use super::error::{ViewError, ViewResult};
use super::{fl, wrap, App, MyLang, Result};
use crate::models::{Tag, Teaser};
use atom_syndication::*;
use std::str::FromStr;
use tracing::instrument;
use warp::filters::BoxedFilter;
use warp::http::header::CONTENT_TYPE;
use warp::http::response::Builder;
use warp::path::{end, param};
use warp::{self, Filter, Reply};

pub fn routes(s: BoxedFilter<(App,)>) -> BoxedFilter<(impl Reply,)> {
    param().and(end()).and(s).then(do_feed).map(wrap).boxed()
}

#[instrument]
async fn do_feed(args: FeedArgs, app: App) -> Result<impl Reply> {
    let db = app.db().await?;

    let tag = if let Some(tag) = args.tag {
        Some(
            db.interact(move |db| Tag::by_slug(&tag, db))
                .await??
                .ok_or(ViewError::NotFound)?,
        )
    } else {
        None
    };

    let fluent = args.lang.fluent()?;
    let lang = args.lang.to_string();
    let tag_id = tag.as_ref().map(|t| t.id);
    let posts = db
        .interact(move |db| {
            if let Some(tag_id) = tag_id {
                Teaser::tagged(tag_id, &lang, 10, db)
            } else {
                Teaser::recent(&lang, 10, db)
            }
        })
        .await??;

    let feed = FeedBuilder::default()
        .title(Text::plain(if let Some(ref tag) = tag {
            fl!(fluent, "taggedhead", tag = AsRef::<str>::as_ref(&tag.name))
        } else {
            fl!(fluent, "sitename")
        }))
        .subtitle(Text::plain(&fl!(fluent, "tagline")))
        .id(if let Some(ref tag) = tag {
            format!("{}/tag/{}.{}", app.base, tag.slug, args.lang)
        } else {
            format!("{}/", app.base)
        })
        .updated(
            posts
                .iter()
                .map(|p| p.updated_at.raw())
                .max()
                .ok_or(ViewError::NotFound)?,
        )
        .entries(
            posts
                .iter()
                .map(|post| {
                    let url = format!("{}{}", app.base, post.url());
                    EntryBuilder::default()
                        .title(post.title.clone())
                        .id(url.clone())
                        .link(
                            LinkBuilder::default().href(url.clone()).build(),
                        )
                        .author(
                            PersonBuilder::default()
                                .name("Rasmus Kaj")
                                .uri(Some(
                                    "https://rasmus.krats.se/rkaj"
                                        .to_string(),
                                ))
                                .build(),
                        )
                        .updated(post.updated_at.raw())
                        .categories(
                            post.tags()
                                .iter()
                                .map(|tag| {
                                    CategoryBuilder::default()
                                        .term(tag.slug.clone())
                                        .label(tag.name.clone())
                                        .build()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .summary(Text::html(format!(
                            "{}\n<p class='readmore'><a href='{}'>{}</a></p>",
                            post.content,
                            url,
                            post.readmore(),
                        )))
                        .published(Some(FixedDateTime::from(
                            post.posted_at.raw(),
                        )))
                        .build()
                })
                .collect::<Vec<_>>(),
        )
        .build();

    Builder::new()
        .header(CONTENT_TYPE, "application/atom+xml")
        .body(feed.to_string())
        .ise()
}

#[derive(Debug)]
struct FeedArgs {
    lang: MyLang,
    tag: Option<String>,
}

impl FromStr for FeedArgs {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use lazy_regex::regex_captures;
        regex_captures!(r"^atom-([a-z]{2})(-([\w-]+))?.xml$", value)
            .and_then(|(_, lang, _, tag)| {
                Some(FeedArgs {
                    lang: lang.parse().ok()?,
                    tag: if tag.is_empty() {
                        None
                    } else {
                        Some(tag.to_string())
                    },
                })
            })
            .ok_or(())
    }
}
