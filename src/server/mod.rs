mod assets;
mod comment;
mod error;
mod feeds;
pub mod language;
mod prelude;
mod tag;

use self::error::{ViewError, ViewResult};
use self::language::AcceptLang;
use self::prelude::*;
use self::templates::RenderRucte;
use crate::dbopt::{Connection, DbOpt, Pool};
use crate::models::{
    year_of_date, Comment, FullPost, MyLang, PostComment, PostTag, Slug, Tag,
    Teaser,
};
use crate::schema::comments::dsl as c;
use crate::schema::metapages::dsl as m;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::PubBaseOpt;
use clap::Parser;
use csrf::{AesGcmCsrfProtection, CsrfCookie, CsrfProtection, CsrfToken};
use diesel::associations::HasTable;
use diesel::dsl::count_distinct;
use diesel::prelude::*;
use diesel::BelongingToDsl;
use diesel_async::pooled_connection::deadpool::PoolError;
use diesel_async::RunQueryDsl;
use reqwest::header::{HeaderMap, CONTENT_SECURITY_POLICY, SERVER};
use serde::Deserialize;
use std::net::SocketAddr;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, instrument, warn};
use warp::filters::BoxedFilter;
use warp::http::header::SET_COOKIE;
use warp::http::response::Builder;
use warp::http::Uri;
use warp::reply::Response;
use warp::{self, header, redirect, Filter, Reply};

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
    csrf_secret: CsrfSecret,

    /// Use this flag if the server runs behind a proxy.
    ///
    /// Comments will then take their source ip address from the
    /// `x-forwarded-for` header instead of the connected remote addr.
    #[clap(long)]
    is_proxied: bool,
}

impl Args {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        use warp::path::{end, param, path};
        use warp::query;
        let quit_sig = async {
            _ = tokio::signal::ctrl_c().await;
            warn!("Initiating graceful shutdown");
        };
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
            .recover(error::for_rejection)
            .boxed();
        let (addr, future) = warp::serve(server)
            .try_bind_with_graceful_shutdown(self.bind, quit_sig)?;
        info!("Running on http://{addr}/");
        future.await;
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
        use base64::prelude::*;
        fn fail<E: std::fmt::Display>(e: E) -> ViewError {
            info!("Csrf verification error: {}", e);
            ViewError::BadRequest("CSRF Verification Failed".into())
        }
        let token = BASE64_STANDARD.decode(token).map_err(fail)?;
        let cookie = BASE64_STANDARD.decode(cookie).map_err(fail)?;
        let protect = self.csrf_protection();
        let token = protect.parse_token(&token).map_err(fail)?;
        let cookie = protect.parse_cookie(&cookie).map_err(fail)?;
        protect
            .verify_token_pair(&token, &cookie)
            .map_err(|e| fail(e.to_string()))
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

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get, head};
    get().or(head()).unify().boxed()
}

fn response() -> Builder {
    Builder::new()
}

/// Create a map of common headers for all served responses.
fn common_headers() -> anyhow::Result<HeaderMap> {
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
    use crate::models::{has_lang, PostLink};
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
        .order((count_distinct(pt::tag_id).desc(), p::posted_at.desc()))
        .limit(8)
        .load(&mut db)
        .await?;

    let (token, cookie) = app.generate_csrf_pair()?;

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

fn found(url: &str) -> impl Reply {
    use warp::http::header;
    use warp::http::StatusCode;
    warp::reply::with_header(StatusCode::FOUND, header::LOCATION, url)
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    c: Option<i32>,
}

#[derive(Clone)]
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
