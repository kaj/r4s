mod assets;
mod comment;
mod csrf;
mod error;
mod feeds;
pub mod language;
mod prelude;
mod tag;

use self::error::{ViewError, ViewResult};
use self::language::AcceptLang;
use self::prelude::*;
use self::templates::RenderRucte;
use crate::PubBaseOpt;
use crate::dbopt::{Connection, DbOpt, Pool};
use crate::models::{
    Comment, FullPost, MyLang, PostComment, PostTag, Slug, Tag, Teaser,
    year_of_date,
};
use crate::schema::comments::dsl as c;
use crate::schema::metapages::dsl as m;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use clap::Parser;
use diesel::BelongingToDsl;
use diesel::associations::HasTable;
use diesel::dsl::count;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::{BuildError, PoolError};
use reqwest::header::{HeaderMap, InvalidHeaderName, InvalidHeaderValue};
use serde::Deserialize;
use std::net::SocketAddr;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, instrument, warn};
use warp::filters::BoxedFilter;
use warp::http::Uri;
use warp::http::header::{CONTENT_SECURITY_POLICY, SERVER, SET_COOKIE};
use warp::http::response::Builder;
use warp::reply::Response;
use warp::{self, Filter, Reply, header, redirect};

type Result<T, E = ViewError> = std::result::Result<T, E>;

#[derive(Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// Adress to listen on
    #[clap(long, default_value = "127.0.0.1:8765")]
    bind: SocketAddr,

    #[clap(flatten)]
    base: PubBaseOpt,

    /// A 32-byte secret key for csrf generation and verification.
    #[clap(long, env = "CSRF_SECRET", hide_env_values = true)]
    csrf_secret: csrf::Secret,

    /// Use this flag if the server runs behind a proxy.
    ///
    /// Comments will then take their source ip address from the
    /// `x-forwarded-for` header instead of the connected remote addr.
    #[clap(long)]
    is_proxied: bool,
}

impl Args {
    pub async fn run(self) -> anyhow::Result<()> {
        use warp::path::{end, param, path};
        use warp::query;
        let app = AppData::new(&self)?;
        let s = warp::any().map(move || app.clone()).boxed();
        let s = move || s.clone();
        let lang_filt = header::optional("accept-language").map(
            |l: Option<AcceptLang>| l.map(|l| l.lang()).unwrap_or_default(),
        );

        let routes = warp::any()
            .and(path("s").and(assets::routes(s())))
            .or(path("comment").and(comment::route(self.is_proxied, s())))
            .or(end()
                .and(goh())
                .and(lang_filt)
                .map(|lang| {
                    format!("/{lang}")
                        .parse::<Uri>()
                        .or_ise()
                        .map(redirect::see_other)
                })
                .boxed())
            .or(path("tag").and(tag::routes(s())).boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(lang_filt)
                .map(|year: i16, lang| {
                    format!("/{year}/{lang}")
                        .parse::<Uri>()
                        .or_ise()
                        .map(redirect::see_other)
                })
                .boxed())
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .then(yearpage)
                .boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(frontpage)
                .boxed())
            .or(param()
                .and(param())
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .then(page)
                .boxed())
            .or(param()
                .and(param())
                .and(end())
                .and(lang_filt)
                .and(goh())
                .and(s())
                .then(page_fallback)
                .boxed())
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(metapage)
                .boxed())
            .or(feeds::routes(s()))
            .or(path("robots.txt").and(end()).and(goh()).map(robots_txt))
            .or(param()
                .and(end())
                .and(lang_filt)
                .and(goh())
                .and(s())
                .then(metafallback)
                .boxed());

        let server = routes
            .with(warp::reply::with::headers(common_headers()?))
            .recover(error::for_rejection);
        let acceptor = TcpListener::bind(self.bind)
            .await
            .map_err(|e| FatalError::Bind(self.bind, e))?;
        if let Ok(addr) = acceptor.local_addr() {
            info!("Running on http://{addr}/");
        }
        warp::serve(server)
            .incoming(acceptor)
            .graceful(quit_sig())
            .run()
            .await;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FatalError {
    #[error("Failed to setup headers: {0}")]
    BadHeader(#[from] BadHeader),
    #[error("Failed to create database pool: {0}")]
    DataPool(#[from] BuildError),
    #[error("Failed to bind {0}: {1:?}")]
    Bind(SocketAddr, std::io::Error),
}

async fn quit_sig() {
    use tokio::signal::{ctrl_c, unix};
    let mut sigterm = unix::signal(unix::SignalKind::terminate()).unwrap();
    let mut sighup = unix::signal(unix::SignalKind::hangup()).unwrap();
    let signal = tokio::select!(
        _ = ctrl_c() => "ctrl-c",
        _ = sighup.recv() => "sighup",
        _ = sigterm.recv() => "sigterm",
    );
    warn!(%signal, "Initiating graceful shutdown");
}

pub struct AppData {
    pool: Pool,
    base: String,
    csrf: csrf::Server,
}
type App = Arc<AppData>;

impl std::fmt::Debug for AppData {
    fn fmt(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.pool.status();
        write!(out, "App(pool {}/{}({}))", s.available, s.size, s.max_size)
    }
}

impl AppData {
    fn new(args: &Args) -> Result<App, BuildError> {
        Ok(Arc::new(AppData {
            pool: args.db.build_pool()?,
            base: args.base.public_base.clone(),
            csrf: csrf::Server::from_key(&args.csrf_secret),
        }))
    }
    async fn db(&self) -> Result<Connection, PoolError> {
        self.pool.get().await
    }
}

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get, head};
    get().or(head()).unify().boxed()
}

fn response() -> Builder {
    Builder::new()
}

/// Create a map of common headers for all served responses.
fn common_headers() -> Result<HeaderMap, BadHeader> {
    // This method is only called once, when initiating the router, so
    // don't bother about performance here.
    Ok(HeaderMap::from_iter([
        (
            SERVER,
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
                .parse()?,
        ),
        (
            CONTENT_SECURITY_POLICY,
            // Note: should use default-src and img-src, but dev server,
            // image server, and lefalet makes that a bit hard.
            "frame-ancestors 'none';".parse()?,
        ),
        ("x-clacks-overhead".parse()?, "GNU Terry Pratchett".parse()?),
    ]))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BadHeader {
    #[error("Bad hedader name: {0}")]
    Name(#[from] InvalidHeaderName),
    #[error("Bad hedader value: {0}")]
    Value(#[from] InvalidHeaderValue),
}

#[instrument]
async fn frontpage(lang: MyLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let limit = 5;
    let posts = Teaser::recent(lang.as_ref(), limit, &mut db).await?;

    let comments = PostComment::recent(&mut db).await?;

    let year = year_of_date(p::posted_at);
    let years = p::posts
        .select(year)
        .distinct()
        .order(year)
        .load(&mut db)
        .await?;

    let other_langs = lang.other(|_, lang, name| {
        format!(
            "<a href='/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
        )});

    Ok(response().html(|o| {
        templates::frontpage_html(
            o,
            lang.fluent(),
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
            lang: lang.parse().map_err(|_| ())?,
        })
    }
}

#[instrument]
async fn yearpage(year: i16, lang: MyLang, app: App) -> Result<impl Reply> {
    let mut db = app.db().await?;
    let posts = Teaser::for_year(year, lang.as_ref(), &mut db).await?;
    if posts.is_empty() {
        return Err(ViewError::NotFound);
    }

    let p_year = year_of_date(p::posted_at);
    let years = p::posts
        .select(p_year)
        .distinct()
        .order(p_year)
        .load(&mut db)
        .await?;

    let fluent = lang.fluent();
    let h1 = fl!(fluent, "posts-year", year = year);
    let other_langs = lang.other(|_, lang, name| {
        format!(
            "<a href='/{year}/{lang}' hreflang='{lang}' lang='{lang}' rel='alternate'>{name}</a>",
        )});

    Ok(response().html(|o| {
        templates::posts_html(
            o,
            fluent,
            &h1,
            None,
            &posts,
            &years,
            &other_langs,
        )
    })?)
}

#[instrument]
async fn page(
    year: i16,
    slug: SlugAndLang,
    query: PageQuery,
    app: App,
) -> Result<Response> {
    use crate::models::{PostLink, has_lang};
    use diesel::dsl::not;
    let mut db = app.db().await?;
    let fluent = slug.lang.fluent();
    let s1 = slug.clone();
    let other_langs = p::posts
        .select((p::lang, p::title))
        .filter(year_of_date(p::posted_at).eq(&year))
        .filter(p::slug.eq(s1.slug.as_ref()))
        .filter(p::lang.ne(s1.lang.as_ref()))
        .load::<(MyLang, String)>(&mut db)
        .await?
        .into_iter()
        .map(|(lang, title)| {
            let fluent = lang.fluent();
            let name = fl!(fluent, "lang-name");
            let title = fl!(fluent, "in-lang", title=title);

            format!(
                "<a href='/{}/{}.{lang}' hreflang='{lang}' lang='{lang}' title='{title}' rel='alternate'>{name}</a>",
                year, slug.slug, lang=lang, title=title, name=name,
            )
        })
        .collect::<Vec<_>>();

    let post = FullPost::load(year, &slug.slug, slug.lang.as_ref(), &mut db)
        .await?
        .ok_or(ViewError::NotFound)?;

    let url = format!("{}{}", app.base, post.url());

    let comments = Comment::belonging_to(&post.deref())
        .select(Comment::as_select())
        .filter(c::is_public)
        .order_by(c::posted_at.asc())
        .load(&mut db)
        .await?;

    let bad_comment = match query.c {
        Some(qc) if comments.iter().any(|c| c.id == qc) => {
            let url = format!("/{year}/{}.{}#c{qc:x}", slug.slug, slug.lang);
            return Ok(found(&url).into_response());
        }
        Some(_) => true,
        None => false,
    };

    let tags = PostTag::belonging_to(post.deref())
        .inner_join(Tag::table())
        .select(Tag::as_select())
        .load(&mut db)
        .await?;

    let tag_ids = tags.iter().map(|t| t.id).collect::<Vec<_>>();

    let lang = post.lang.as_ref();
    let p_year = year_of_date(p::posted_at);
    let related = PostLink::all()
        .group_by(p::id)
        .filter(p::id.ne(post.id))
        .filter(p::lang.eq(lang).or(not(has_lang(p_year, p::slug, lang))))
        .left_join(pt::post_tags.on(p::id.eq(pt::post_id)))
        .filter(pt::tag_id.eq_any(tag_ids))
        .order((
            count(pt::tag_id).aggregate_distinct().desc(),
            p::posted_at.desc(),
        ))
        .limit(8)
        .load(&mut db)
        .await?;

    let (token, cookie) = app.csrf.generate_pair()?;

    Ok(response()
        .header(
            SET_COOKIE,
            format!(
                "CSRF={}; SameSite=Strict; Path=/; Secure; HttpOnly",
                cookie.b64_string()
            ),
        )
        .html(|o| {
            templates::post_html(
                o,
                fluent,
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

/// When asked for a page without lang in url, redirect to existing.
///
/// Tries to respect the language preference from the user agent.
#[instrument]
async fn page_fallback(
    year: i16,
    slug: Slug,
    lpref: MyLang,
    app: App,
) -> Result<impl Reply> {
    let mut db = app.db().await?;

    let slugc = slug.clone();
    let lang = p::posts
        .select(p::lang)
        .filter(year_of_date(p::posted_at).eq(&year))
        .filter(p::slug.eq(slugc.as_ref()))
        .order(p::lang.eq(lpref.as_ref()).desc())
        .first::<String>(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;

    Ok(found(&format!("/{year}/{slug}.{lang}")))
}

#[instrument]
async fn metapage(slug: SlugAndLang, app: App) -> Result<Response> {
    let mut db = app.db().await?;
    let fluent = slug.lang.fluent();
    let s1 = slug.clone();
    let other_langs = m::metapages
        .select((m::lang, m::title))
        .filter(m::slug.eq(s1.slug.as_ref()))
        .filter(m::lang.ne(s1.lang.as_ref()))
        .load::<(MyLang, String)>(&mut db)
        .await?
        .into_iter()
        .map(|(lang, title)| {
            let fluent = lang.fluent();
            let name = fl!(fluent, "lang-name");
            let title = fl!(fluent, "in-lang", title=title);

            format!(
                "<a href='/{}.{lang}' hreflang='{lang}' lang='{lang}' title='{title}' rel='alternate'>{name}</a>",
                slug.slug, lang=lang, title=title, name=name,
            )
        })
        .collect::<Vec<_>>();

    let (title, content) = m::metapages
        .select((m::title, m::content))
        .filter(m::slug.eq(slug.slug.as_ref()))
        .filter(m::lang.eq(slug.lang.as_ref()))
        .first::<(String, String)>(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;

    Ok(response().html(|o| {
        templates::page_html(o, fluent, &title, &content, &other_langs)
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
        let existing_langs = m::metapages
            .select(m::lang)
            .filter(m::slug.eq(s1))
            .load::<String>(&mut app.db().await?)
            .await?;

        if existing_langs.is_empty() {
            Err(ViewError::NotFound)
        } else {
            let lang = existing_langs
                .iter()
                .find(|l| lang.as_ref() == *l)
                .unwrap_or(&existing_langs[0]);
            Ok(found(&format!("/{slug}.{lang}")))
        }
    }
}

fn found(url: &str) -> impl Reply + use<> {
    use warp::http::StatusCode;
    use warp::http::header;
    warp::reply::with_header(StatusCode::FOUND, header::LOCATION, url)
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    c: Option<i32>,
}

fn robots_txt() -> Result<Response> {
    use warp::http::header::CONTENT_TYPE;
    response()
        .header(CONTENT_TYPE, mime::TEXT_PLAIN.as_ref())
        .body(
            "User-agent: *\n\
             Disallow: /tmp/\n"
                .into(),
        )
        .or_ise()
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
