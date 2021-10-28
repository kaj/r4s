mod error;
pub mod language;
mod tag;

use self::error::ViewError;
use self::templates::RenderRucte;
use crate::dbopt::{DbOpt, Pool};
use crate::models::{year_of_date, Post, Tag};
use crate::schema::posts::dsl as p;
use accept_language::intersection;
use diesel::prelude::*;
use std::net::SocketAddr;
use std::str::FromStr;
use structopt::StructOpt;
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::http::Uri;
use warp::path::Tail;
use warp::reply::Response;
use warp::{self, Filter, Rejection, Reply};
use warp::{header, redirect};

type Result<T, E = ViewError> = std::result::Result<T, E>;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// Adress to listen on
    #[structopt(long, default_value = "127.0.0.1:8765")]
    bind: SocketAddr,
}

impl Args {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        use warp::path::{end, param, path, tail};
        let pool = self.db.build_pool()?;
        let s = warp::any().map(move || pool.clone()).boxed();
        let s = move || s.clone();
        let lang_filt = header::optional("accept-language")
            .map(Option::unwrap_or_default);
        let routes = warp::any()
            .and(path("s").and(tail()).and(goh()).and_then(static_file))
            .or(end().and(goh()).and(lang_filt).map(|lang: MyLang| {
                redirect::see_other(
                    Uri::builder()
                        .path_and_query(&format!("/{}", lang.0))
                        .build()
                        .unwrap(),
                )
            }))
            .or(path("tag").and(tag::routes(s())))
            .or(param().and(end()).and(goh()).and(lang_filt).map(
                |year: i16, lang: MyLang| {
                    redirect::see_other(
                        Uri::builder()
                            .path_and_query(&format!("/{}/{}", year, lang.0))
                            .build()
                            .unwrap(),
                    )
                },
            ))
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .and_then(
                    |y, l, a| async move { wrap(yearpage(y, l, a).await) },
                ))
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .and_then(|l, a| async { wrap(frontpage(l, a).await) }))
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .and_then(move |a, y, s| async move {
                    wrap(page(a, y, s).await)
                }));

        warp::serve(routes).run(self.bind).await;
        Ok(())
    }
}

/// Either "sv" or "en".
#[derive(Debug)]
struct MyLang(String);

impl FromStr for MyLang {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, ()> {
        Ok(MyLang(
            intersection(value, vec!["en", "sv"])
                .drain(..)
                .next()
                .ok_or(())?,
        ))
    }
}
impl Default for MyLang {
    fn default() -> Self {
        MyLang("en".into())
    }
}

fn wrap(result: Result<impl Reply>) -> Result<Response, Rejection> {
    match result {
        Ok(reply) => Ok(reply.into_response()),
        Err(err) => Ok(err.into_response()),
    }
}

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get, head};
    get().or(head()).unify().boxed()
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
async fn static_file(_name: Tail) -> Result<impl Reply, Rejection> {
    Ok("todo")
    /*
    use crate::templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Ok(Builder::new()
            .header(CONTENT_TYPE, data.mime.as_ref())
            .header(EXPIRES, far_expires.to_rfc2822())
            .body(data.content))
    } else {
        log::info!("Static file {:?} not found", name);
        Err(not_found())
    }
    */
}

async fn frontpage(lang: MyLang, pool: Pool) -> Result<Response> {
    let db = pool.get().await?;
    let fluent = language::load(&lang.0)?;
    let limit = 5;
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
                    sql::<Bool>(&format!("bool_or(lang='{}') over (partition by year_of_date(posted_at), slug)", lang.0))
                ))
                .order(p::updated_at.desc())
                .limit(2 * limit as i64)
                .load::<(Post, bool)>(db)?
                .into_iter()
                .filter_map(|(post, langq)| {
                    if post.lang == lang.0 || !langq {
                        Some(post)
                    } else {
                        None
                    }
                })
                .take(limit)
                .map(|post| {
                    Tag::for_post(post.id, db).map(|tags| (post, tags))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .await??;

    let years = db
        .interact(move |db| {
            let year = year_of_date(p::posted_at);
            p::posts.select(year).distinct().order(year).load(db)
        })
        .await??;

    Ok(Builder::new()
        .html(|o| templates::frontpage(o, &fluent, &posts, &years))
        .unwrap())
}

#[derive(Debug, Clone)]
struct SlugAndLang {
    slug: String,
    lang: String,
}

impl FromStr for SlugAndLang {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (slug, lang) = s.split_once('.').ok_or(())?;
        // TODO: check "slug rules"
        Ok(SlugAndLang {
            slug: slug.into(),
            lang: lang.into(),
        })
    }
}

async fn yearpage(year: i16, lang: MyLang, pool: Pool) -> Result<impl Reply> {
    let db = pool.get().await?;
    let fluent = language::load(&lang.0)?;
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
                    sql::<Bool>(&format!("bool_or(lang='{}') over (partition by year_of_date(posted_at), slug)", lang.0))
                ))
                .filter(year_of_date(p::posted_at).eq(year))
                .order(p::updated_at.desc())
                .load::<(Post, bool)>(db)
                .and_then(|data| data.into_iter()
                .filter_map(|(post, langq)| {
                    if post.lang == lang.0 || !langq {
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

    let years = db
        .interact(move |db| {
            let year = year_of_date(p::posted_at);
            p::posts.select(year).distinct().order(year).load(db)
        })
        .await??;

    Ok(Builder::new()
        .html(|o| templates::frontpage(o, &fluent, &posts, &years))
        .unwrap())
}

async fn page(year: i16, slug: SlugAndLang, pool: Pool) -> Result<Response> {
    let db = pool.get().await?;
    let fluent = language::load(&slug.lang)?;
    let s1 = slug.clone();
    let other_langs = db
        .interact(move |db| {
            p::posts
                .select(p::lang)
                .filter(year_of_date(p::posted_at).eq(&year))
                .filter(p::slug.eq(s1.slug))
                .filter(p::lang.ne(s1.lang))
                .load::<String>(db)
        })
        .await??
        .into_iter()
        .map(|lang| format!(
            "<a href='/{}/{}.{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{lang}</a>",
            year, slug.slug, lang=lang,
        ))
        .collect::<Vec<_>>();

    let post = db
        .interact(move |db| {
            p::posts
                .select((
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::content,
                ))
                .filter(year_of_date(p::posted_at).eq(&year))
                .filter(p::slug.eq(slug.slug))
                .filter(p::lang.eq(slug.lang))
                .first::<Post>(db)
                .optional()
        })
        .await??
        .ok_or(ViewError::NotFound)?;

    let post_id = post.id;
    let tags = db.interact(move |db| Tag::for_post(post_id, db)).await??;

    Ok(Builder::new()
        .html(|o| templates::page(o, fluent, &post, &tags, &other_langs))
        .unwrap())
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
