mod comment;
mod error;
mod feeds;
pub mod language;
mod prelude;
mod tag;

use self::error::{ViewError, ViewResult};
use self::language::{AcceptLang, MyLang};
use self::prelude::*;
use self::templates::RenderRucte;
use crate::dbopt::{Connection, DbOpt, Pool};
use crate::models::{
    year_of_date, Comment, FullPost, PostComment, Slug, Tag, Teaser,
};
use crate::schema::assets::dsl as a;
use crate::schema::metapages::dsl as m;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::PubBaseOpt;
use csrf::{AesGcmCsrfProtection, CsrfCookie, CsrfProtection, CsrfToken};
use deadpool_diesel::PoolError;
use diesel::prelude::*;
use serde::Deserialize;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use structopt::StructOpt;
use tracing::{event, instrument, Level};
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::http::Uri;
use warp::path::Tail;
use warp::reply::Response;
use warp::{self, header, redirect, Filter, Reply};

pub use templates::ToHtml;

type Result<T, E = ViewError> = std::result::Result<T, E>;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// Adress to listen on
    #[structopt(long, default_value = "127.0.0.1:8765")]
    bind: SocketAddr,

    #[structopt(flatten)]
    base: PubBaseOpt,

    /// A 32-byte secret key for csrf generation and verification.
    #[structopt(long, env = "CSRF_SECRET", hide_env_values = true)]
    csrf_secret: CsrfSecret,

    /// Use this flag if the server runs behind a proxy.
    ///
    /// Comments will then take their source ip address from the
    /// `x-forwarded-for` header instead of the connected remote addr.
    #[structopt(long)]
    is_proxied: bool,
}

impl Args {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        use warp::path::{end, param, path, tail};
        use warp::query;
        let app = AppData::new(&self)?;
        let s = warp::any().map(move || app.clone()).boxed();
        let s = move || s.clone();
        let lang_filt = header::optional("accept-language").map(
            |l: Option<AcceptLang>| l.map(|l| l.lang()).unwrap_or_default(),
        );
        let asset_routes = goh()
            .and(param())
            .and(param())
            .and(end())
            .and(goh())
            .and(s())
            .then(asset_file)
            .map(wrap)
            .or(tail().and(goh()).map(static_file).map(wrap))
            .unify();

        let routes = warp::any()
            .and(path("s").and(asset_routes).boxed())
            .or(path("comment").and(comment::route(self.is_proxied, s())))
            .or(end()
                .and(goh())
                .and(lang_filt)
                .map(|lang| {
                    wrap(
                        Uri::builder()
                            .path_and_query(&format!("/{}", lang))
                            .build()
                            .or_ise()
                            .map(redirect::see_other),
                    )
                })
                .boxed())
            .or(path("tag").and(tag::routes(s())).boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(lang_filt)
                .map(|year: i16, lang| {
                    wrap(
                        Uri::builder()
                            .path_and_query(&format!("/{}/{}", year, lang))
                            .build()
                            .map(redirect::see_other)
                            .or_ise(),
                    )
                })
                .boxed())
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .then(yearpage)
                .map(wrap)
                .boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(frontpage)
                .map(wrap)
                .boxed())
            .or(param()
                .and(param())
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .then(page)
                .map(wrap)
                .boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(metapage)
                .map(wrap)
                .boxed())
            .or(feeds::routes(s()))
            .or(param()
                .and(end())
                .and(lang_filt)
                .and(goh())
                .and(s())
                .then(metafallback)
                .map(wrap)
                .boxed());

        warp::serve(routes.recover(error::for_rejection).boxed())
            .run(self.bind)
            .await;
        Ok(())
    }
}

pub struct AppData {
    pool: Pool,
    base: String,
    csrf_secret: [u8; 32],
}
type App = Arc<AppData>;

impl std::fmt::Debug for AppData {
    fn fmt(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.pool.status();
        write!(out, "App(pool {}/{}({}))", s.available, s.size, s.max_size)
    }
}

impl AppData {
    fn new(args: &Args) -> Result<App, anyhow::Error> {
        Ok(Arc::new(AppData {
            pool: args.db.build_pool()?,
            base: args.base.public_base.clone(),
            csrf_secret: args.csrf_secret.secret,
        }))
    }
    async fn db(&self) -> Result<Connection, PoolError> {
        self.pool.get().await
    }
    fn verify_csrf(&self, token: &str, cookie: &str) -> Result<()> {
        use base64::decode;
        fn fail<E: std::fmt::Display>(e: E) -> ViewError {
            event!(Level::INFO, "Csrf verification error: {}", e);
            ViewError::BadRequest("CSRF Verification Failed".into())
        }
        let token = decode(token.as_bytes()).map_err(fail)?;
        let cookie = decode(cookie.as_bytes()).map_err(fail)?;
        let protect = self.csrf_protection();
        let token = protect.parse_token(&token).map_err(fail)?;
        let cookie = protect.parse_cookie(&cookie).map_err(fail)?;
        if protect.verify_token_pair(&token, &cookie) {
            Ok(())
        } else {
            Err(fail("invalid pair"))
        }
    }
    fn generate_csrf_pair(&self) -> Result<(CsrfToken, CsrfCookie)> {
        let ttl = 4 * 3600;
        self.csrf_protection()
            .generate_token_pair(None, ttl)
            .or_ise()
    }
    fn csrf_protection(&self) -> impl CsrfProtection {
        AesGcmCsrfProtection::from_key(self.csrf_secret)
    }
}

fn wrap(result: Result<impl Reply>) -> Response {
    match result {
        Ok(reply) => reply.into_response(),
        Err(err) => err.into_response(),
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
#[instrument]
fn static_file(name: Tail) -> Result<impl Reply> {
    use chrono::{Duration, Utc};
    use templates::statics::StaticFile;
    use warp::http::header::{CONTENT_TYPE, EXPIRES};
    let data = StaticFile::get(name.as_str()).ok_or(ViewError::NotFound)?;
    let far_expires = Utc::now() + Duration::days(180);
    Builder::new()
        .header(CONTENT_TYPE, data.mime.as_ref())
        .header(EXPIRES, far_expires.to_rfc2822())
        .body(data.content)
        .or_ise()
}

#[instrument]
async fn asset_file(year: i16, name: String, app: App) -> Result<Response> {
    use warp::http::header::CONTENT_TYPE;
    let db = app.db().await?;

    let (mime, content) = db
        .interact(move |db| {
            a::assets
                .select((a::mime, a::content))
                .filter(a::year.eq(year))
                .filter(a::name.eq(name))
                .first::<(String, Vec<u8>)>(db)
                .optional()
        })
        .await??
        .ok_or(ViewError::NotFound)?;

    Builder::new()
        .header(CONTENT_TYPE, mime)
        //.header(EXPIRES, far_expires.to_rfc2822())
        .body(content.into())
        .or_ise()
}

#[instrument]
async fn frontpage(lang: MyLang, app: App) -> Result<Response> {
    let db = app.db().await?;
    let limit = 5;
    let langc = lang.clone();
    let posts = db
        .interact(move |db| Teaser::recent(lang.as_ref(), limit, db))
        .await??;

    let comments = db.interact(move |db| PostComment::recent(db)).await??;

    let years = db
        .interact(move |db| {
            let year = year_of_date(p::posted_at);
            p::posts.select(year).distinct().order(year).load(db)
        })
        .await??;

    let fluent = langc.fluent()?;
    let other_langs = langc.other(|_, lang, name| {
        format!(
            "<a href='/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
            lang=lang, name=name,
        )});

    Ok(Builder::new().html(|o| {
        templates::frontpage(
            o,
            &fluent,
            &posts,
            &comments,
            &years,
            &other_langs,
        )
    })?)
}

#[derive(Debug, Clone)]
struct SlugAndLang {
    slug: Slug,
    lang: MyLang,
}
impl FromStr for SlugAndLang {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (slug, lang) = s.split_once('.').ok_or(())?;
        Ok(SlugAndLang {
            slug: slug.parse()?,
            lang: lang.parse()?,
        })
    }
}

#[instrument]
async fn yearpage(year: i16, lang: MyLang, app: App) -> Result<impl Reply> {
    let db = app.db().await?;
    let langc = lang.clone();
    let posts = db
        .interact(move |db| Teaser::for_year(year, lang.as_ref(), db))
        .await??;
    if posts.is_empty() {
        return Err(ViewError::NotFound);
    }

    let years = db
        .interact(move |db| {
            let year = year_of_date(p::posted_at);
            p::posts.select(year).distinct().order(year).load(db)
        })
        .await??;

    let fluent = langc.fluent()?;
    let h1 = fl!(fluent, "posts-year", year = year);
    let other_langs = langc.other(|_, lang, name| {
        format!(
            "<a href='/{}/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
            year, lang=lang, name=name,
        )});

    Ok(Builder::new().html(|o| {
        templates::posts(o, &fluent, &h1, None, &posts, &years, &other_langs)
    })?)
}

#[instrument]
async fn page(
    year: i16,
    slug: SlugAndLang,
    query: PageQuery,
    app: App,
) -> Result<Response> {
    use crate::models::{has_lang, PostLink};
    use diesel::dsl::not;
    let db = app.db().await?;
    let fluent = slug.lang.fluent()?;
    let s1 = slug.clone();
    let other_langs = db
        .interact(move |db| {
            p::posts
                .select((p::lang, p::title))
                .filter(year_of_date(p::posted_at).eq(&year))
                .filter(p::slug.eq(s1.slug.as_ref()))
                .filter(p::lang.ne(s1.lang.as_ref()))
                .load::<(String, String)>(db)
        })
        .await??
        .into_iter()
        .map(|(lang, title)| {
            let fluent = language::load(&lang).unwrap();
            let name = fl!(fluent, "lang-name");
            let title = fl!(fluent, "in-lang", title=title);

            format!(
                "<a href='/{}/{}.{lang}' hreflang='{lang}' lang='{lang}' title='{title}' rel='alternate'>{name}</a>",
                year, slug.slug, lang=lang, title=title, name=name,
            )
        })
        .collect::<Vec<_>>();

    let slugc = slug.clone();
    let post = db
        .interact(move |db| {
            FullPost::load(year, &slug.slug, slug.lang.as_ref(), db)
        })
        .await??
        .ok_or(ViewError::NotFound)?;

    let url = format!("{}{}", app.base, post.url());

    let post_id = post.id;
    let comments = db
        .interact(move |db| Comment::for_post(post_id, db))
        .await??;

    let bad_comment = if let Some(q_comment) = query.c {
        for cmt in &comments {
            if cmt.id == q_comment {
                return Ok(found(&format!(
                    "/{}/{}.{}#c{:x}",
                    year, slugc.slug, slugc.lang, q_comment,
                ))
                .into_response());
            }
        }
        true
    } else {
        false
    };

    let post_id = post.id;
    let tags = db.interact(move |db| Tag::for_post(post_id, db)).await??;

    let tag_ids = tags.iter().map(|t| t.id).collect::<Vec<_>>();
    let post_id = post.id;
    let lang = post.lang.clone();
    let related = db
        .interact(move |db| {
            use diesel::dsl::sql;
            use diesel::sql_types::BigInt;
            let c = sql::<BigInt>("count(*)");
            let post_fields = (
                p::id,
                year_of_date(p::posted_at),
                p::slug,
                p::lang,
                p::title,
            );
            p::posts
                .select(post_fields)
                .left_join(pt::post_tags.on(p::id.eq(pt::post_id)))
                .filter(pt::tag_id.eq_any(tag_ids))
                .filter(p::lang.eq(&lang).or(not(has_lang(
                    year_of_date(p::posted_at),
                    p::slug,
                    &lang,
                ))))
                .filter(p::id.ne(post_id))
                .group_by(post_fields)
                .order((c.desc(), p::posted_at.desc()))
                .limit(8)
                .load::<PostLink>(db)
        })
        .await??;

    let (token, cookie) = app.generate_csrf_pair()?;

    use warp::http::header::SET_COOKIE;
    Ok(Builder::new()
        .header(
            SET_COOKIE,
            format!(
                "CSRF={}; SameSite=Strict; Path=/; HttpOnly",
                cookie.b64_string()
            ),
        )
        .html(|o| {
            templates::post(
                o,
                &fluent,
                &url,
                &post,
                &tags,
                bad_comment,
                &token.b64_string(),
                &comments,
                &other_langs,
                &related,
            )
        })?)
}

#[instrument]
async fn metapage(slug: SlugAndLang, app: App) -> Result<Response> {
    let db = app.db().await?;
    let fluent = slug.lang.fluent()?;
    let s1 = slug.clone();
    let other_langs = db
        .interact(move |db| {
            m::metapages
                .select((m::lang, m::title))
                .filter(m::slug.eq(s1.slug.as_ref()))
                .filter(m::lang.ne(s1.lang.as_ref()))
                .load::<(String, String)>(db)
        })
        .await??
        .into_iter()
        .map(|(lang, title): (String, String)| {
            let fluent = language::load(&lang).unwrap();
            let name = fl!(fluent, "lang-name");
            let title = fl!(fluent, "in-lang", title=title);

            format!(
                "<a href='/{}.{lang}' hreflang='{lang}' lang='{lang}' title='{title}' rel='alternate'>{name}</a>",
                slug.slug, lang=lang, title=title, name=name,
            )
        })
        .collect::<Vec<_>>();

    let (title, content) = db
        .interact(move |db| {
            m::metapages
                .select((m::title, m::content))
                .filter(m::slug.eq(slug.slug.as_ref()))
                .filter(m::lang.eq(slug.lang.as_ref()))
                .first::<(String, String)>(db)
                .optional()
        })
        .await??
        .ok_or(ViewError::NotFound)?;

    Ok(Builder::new().html(|o| {
        templates::page(o, &fluent, &title, &content, &other_langs)
    })?)
}

#[instrument]
async fn metafallback(
    slug: String,
    lang: MyLang,
    app: App,
) -> Result<impl Reply> {
    if slug == "about" {
        Ok(found("/site.en"))
    } else if slug == "RasmusKaj" {
        Ok(found("/rkaj.en"))
    } else {
        let s1 = slug.clone();
        let existing_langs = app
            .db()
            .await?
            .interact(move |db| {
                m::metapages
                    .select(m::lang)
                    .filter(m::slug.eq(s1))
                    .load::<String>(db)
            })
            .await??;

        if existing_langs.is_empty() {
            Err(ViewError::NotFound)
        } else {
            let lang = existing_langs
                .iter()
                .find(|l| lang.as_ref() == *l)
                .unwrap_or(&existing_langs[0]);
            Ok(found(&format!("/{}.{}", slug, lang)))
        }
    }
}

fn found(url: &str) -> impl Reply {
    use warp::http::header;
    use warp::http::StatusCode;
    warp::reply::with_header(StatusCode::FOUND, header::LOCATION, url)
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    c: Option<i32>,
}

struct CsrfSecret {
    secret: [u8; 32],
}

impl FromStr for CsrfSecret {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(CsrfSecret {
            secret: s.as_bytes().try_into().map_err(|_| {
                anyhow::anyhow!("Got {} bytes, expected 32", s.len())
            })?,
        })
    }
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
