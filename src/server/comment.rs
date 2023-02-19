use super::error::{ViewError, ViewResult};
use super::{wrap, App, Result};
use crate::models::{safe_md2html, PostLink};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use ipnetwork::IpNetwork;
use reqwest::Url;
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use tracing::instrument;
use warp::filters::{addr, cookie, header, BoxedFilter};
use warp::path::end;
use warp::reject::{reject, Rejection};
use warp::{self, body, post, Filter, Reply};

pub fn route(
    proxied: bool,
    s: BoxedFilter<(App,)>,
) -> BoxedFilter<(impl Reply,)> {
    end()
        .and(post())
        .and(remote_addr_filter(proxied))
        .and(cookie::cookie("CSRF"))
        .and(body::form())
        .and(s)
        .then(postcomment)
        .map(wrap)
        .boxed()
}

#[instrument]
async fn postcomment(
    ip: IpAddr,
    csrf_cookie: String,
    form: CommentForm,
    app: App,
) -> Result<impl Reply> {
    app.verify_csrf(&form.csrftoken, &csrf_cookie)?;
    let mut db = app.db().await?;

    let post = PostLink::all()
        .filter(p::id.eq(form.post))
        .first(&mut db)
        .await?;

    let url = form
        .url
        .as_ref()
        .filter(|u| !u.trim().is_empty())
        .map(|u| {
            Url::parse(u).map_err(|e| {
                tracing::info!("Invalid url {:?}: {}", u, e);
                ViewError::BadRequest("Bad url".into())
            })
        })
        .transpose()?;
    let counts = c::comments
        .group_by((c::is_public, c::is_spam))
        .select(((c::is_public, c::is_spam), count_star()))
        .filter(c::name.eq(&form.name))
        .filter(c::email.eq(&form.email))
        .load::<((bool, bool), i64)>(&mut db)
        .await?;
    let mut public = 0;
    let mut spam = 0;
    for ((is_public, is_spam), count) in counts {
        if is_spam {
            spam += count;
        } else if is_public {
            public += count;
        }
    }
    if spam > 0 {
        tracing::info!("There are {} simliar spam posts.  Reject.", spam);
        return Err(ViewError::BadRequest("This seems like spam".into()));
    }
    let public = public > 0;

    let (id, public) = diesel::insert_into(c::comments)
        .values((
            c::post_id.eq(&form.post),
            c::content.eq(form.html()),
            c::name.eq(&form.name),
            c::email.eq(&form.email),
            url.as_ref().map(|u| c::url.eq(u.as_str())),
            c::from_host.eq(IpNetwork::from(ip)),
            c::raw_md.eq(&form.comment),
            c::is_public.eq(public),
        ))
        .returning((c::id, c::is_public))
        .get_result::<(i32, bool)>(&mut db)
        .await?;

    tracing::info!("Comment accepted.  Public? {}", public);
    my_found(&post, public, id)
}

pub fn my_found(
    post: &PostLink,
    public: bool,
    comment: i32,
) -> Result<impl Reply> {
    use std::fmt::Write;
    let mut url = post.url();
    if public {
        write!(&mut url, "#c{:x}", comment).or_ise()?
    } else {
        write!(&mut url, "?c={}#cxmod", comment).or_ise()?
    }
    Ok(super::found(&url))
}

#[derive(Debug, Deserialize)]
struct CommentForm {
    post: i32,
    comment: String,
    name: String,
    email: String,
    url: Option<String>,
    csrftoken: String,
}

impl CommentForm {
    fn html(&self) -> String {
        safe_md2html(&self.comment)
    }
}

fn remote_addr_filter(proxied: bool) -> BoxedFilter<(IpAddr,)> {
    if proxied {
        header::header("x-forwarded-for").boxed()
    } else {
        addr::remote().and_then(sa2ip).boxed()
    }
}

async fn sa2ip(sockaddr: Option<SocketAddr>) -> Result<IpAddr, Rejection> {
    sockaddr.map(|s| s.ip()).ok_or_else(reject)
}
