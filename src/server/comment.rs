use super::error::{ViewError, ViewResult};
use super::{wrap, App, Result};
use crate::models::{safe_md2html, PostLink};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use diesel::dsl::sql;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
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
    let db = app.db().await?;

    let post = form.post;
    let post = db
        .interact(move |db| {
            PostLink::select()
                .filter(p::id.eq(post))
                .first::<PostLink>(db)
        })
        .await??;

    let name = form.name.clone();
    let email = form.email.clone();
    let (public, spam) = db
        .interact(move |db| {
            c::comments
                .select(((c::is_public, c::is_spam), sql("count(*)")))
                .group_by((c::is_public, c::is_spam))
                .filter(c::name.eq(name))
                .filter(c::email.eq(email))
                .load::<((bool, bool), i64)>(db)
        })
        .await?
        .map(|raw| {
            let mut public = 0;
            let mut spam = 0;
            for ((is_public, is_spam), count) in raw {
                if is_spam {
                    spam += count;
                } else if is_public {
                    public += count;
                }
            }
            (public, spam)
        })?;
    if spam > 0 {
        return Err(ViewError::BadRequest("This seems like spam".into()));
    }
    let public = public > 0;

    let (id, public) = db
        .interact(move |db| {
            diesel::insert_into(c::comments)
                .values((
                    c::post_id.eq(&form.post),
                    c::content.eq(form.html()),
                    c::name.eq(&form.name),
                    c::email.eq(&form.email),
                    form.url.as_ref().map(|u| c::url.eq(u)),
                    c::from_host.eq(IpNetwork::from(ip)),
                    c::raw_md.eq(&form.comment),
                    c::is_public.eq(public),
                ))
                .returning((c::id, c::is_public))
                .get_result::<(i32, bool)>(db)
        })
        .await??;

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
        write!(&mut url, "#c{:x}", comment).ise()?
    } else {
        write!(&mut url, "?c={}#cxmod", comment).ise()?
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
