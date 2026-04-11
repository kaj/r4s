use super::error::ViewError;
use super::{App, Result};
use crate::models::{DateTime, PostLink, safe_md2html};
use crate::schema::comments::dsl as c;
use crate::schema::posts::{self, dsl as p};
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use ipnetwork::IpNetwork;
use reqwest::Url;
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr};
use tracing::{Level, instrument};
use warp::filters::{BoxedFilter, cookie, header};
use warp::path::end;
use warp::reply::Response;
use warp::{self, Filter, Reply, body, post};

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
        .boxed()
}

#[instrument(err(level = Level::WARN))]
async fn postcomment(
    ip: IpAddr,
    csrf_cookie: String,
    form: CommentForm,
    app: App,
) -> Result<Response> {
    app.csrf.verify(&form.csrftoken, &csrf_cookie)?;
    let form = form.validate()?;
    let mut db = app.db().await?;

    let (post, updated) = posts::table
        .select((PostLink::as_select(), p::updated_at))
        .filter(p::id.eq(form.post))
        .first::<(PostLink, DateTime)>(&mut db)
        .await?;

    if updated.old_age().is_some() {
        tracing::info!(post = post.url(), "Reject comment on old post.");
        return Err(ViewError::BadRequest(
            "This post is too old to comment.".into(),
        ));
    }

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
            form.url.as_ref().map(|u| c::url.eq(u)),
            c::from_host.eq(IpNetwork::from(ip)),
            c::raw_md.eq(&form.comment),
            c::is_public.eq(public),
        ))
        .returning((c::id, c::is_public))
        .get_result::<(i32, bool)>(&mut db)
        .await?;

    tracing::info!("Comment accepted.  Public? {}", public);
    Ok(my_found(&post, public, id))
}

pub fn my_found(post: &PostLink, public: bool, comment: i32) -> Response {
    let url = post.url();
    super::found(&if public {
        format!("{url}#c{comment:x}")
    } else {
        format!("{url}?c={comment}#cxmod")
    })
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct CommentForm {
    post: i32,
    comment: String,
    name: String,
    email: String,
    url: Option<String>,
    csrftoken: String,
}

impl CommentForm {
    /// Simple validation of comment form data.
    fn validate(mut self) -> Result<Self, BadForm> {
        self.email = self.email.trim().into();
        self.name = self.name.trim().into();

        if self.email.rsplit_once('@').is_none_or(|(before, after)| {
            before.is_empty()
                || before.chars().any(|c| c.is_control() || c.is_whitespace())
                || after.is_empty()
                || after.chars().any(|c| c.is_control() || c.is_whitespace())
        }) {
            return Err(BadForm::Email);
        }

        if self.name.is_empty() || self.name.chars().any(|c| c.is_control()) {
            return Err(BadForm::Name);
        }

        self.url = self
            .url
            .filter(|u| !u.trim().is_empty())
            .map(|u| {
                let u = Url::parse(&u).map_err(|e| e.to_string())?;

                let scheme = dbg!(&u).scheme();
                if scheme != "http" && scheme != "https" {
                    return Err("Must be https or http.".into());
                }
                let host = u.host_str().ok_or("An url needs a host")?;

                if URLSHORTENERS.contains(&host) {
                    return Err("Please use a non-shortened url".into());
                }
                Ok(u.to_string())
            })
            .transpose()
            .map_err(BadForm::Url)?;

        Ok(self)
    }

    fn html(&self) -> String {
        safe_md2html(&self.comment)
    }
}

#[derive(Debug, Eq, PartialEq)]
enum BadForm {
    Email,
    Name,
    Url(String),
}

const URLSHORTENERS: &[&str] = &[
    "bit.ly",
    "coub.com",
    "cutt.ly",
    "g.page",
    "pca.st",
    "short-url.org",
    "t.ly",
    "tinyurl.com",
];

impl std::error::Error for BadForm {}
impl std::fmt::Display for BadForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Email => f.write_str("A valid email is required."),
            Self::Name => f.write_str("Some kind of name is required."),
            Self::Url(msg) => write!(f, "Bad url: {msg}"),
        }
    }
}
impl From<BadForm> for ViewError {
    fn from(value: BadForm) -> Self {
        ViewError::BadRequest(value.to_string())
    }
}

fn remote_addr_filter(proxied: bool) -> BoxedFilter<(IpAddr,)> {
    if proxied {
        header::header("x-forwarded-for").boxed()
    } else {
        warp::filters::any::any()
            .map(|| {
                tracing::warn!("Executed remote_addr_filter without proxy.");
                IpAddr::from(Ipv4Addr::from([127, 0, 0, 1]))
            })
            .boxed()
    }
}

#[cfg(test)]
mod test {
    use super::{BadForm, CommentForm};

    impl Default for CommentForm {
        fn default() -> Self {
            Self {
                post: 17,
                comment: "This is a comment".into(),
                name: "Rasmus Kaj".into(),
                email: "rasmus@krats.se".into(),
                url: None,
                csrftoken: "xyzzy".into(),
            }
        }
    }

    #[test]
    pub fn simple_ok() {
        let form = CommentForm::default();
        assert_eq!(form.validate().unwrap(), CommentForm::default());
    }

    #[test]
    pub fn no_name() {
        let form = CommentForm {
            name: String::new(),
            ..Default::default()
        };
        assert_eq!(form.validate(), Err(BadForm::Name));
    }

    #[test]
    pub fn no_email() {
        let form = CommentForm {
            email: String::new(),
            ..Default::default()
        };
        assert_eq!(form.validate(), Err(BadForm::Email));
    }

    #[test]
    pub fn bad_email_1() {
        let form = CommentForm {
            email: "kalle".into(),
            ..Default::default()
        };
        assert_eq!(form.validate(), Err(BadForm::Email));
    }
    #[test]
    pub fn bad_email_2() {
        let form = CommentForm {
            email: "kalle@".into(),
            ..Default::default()
        };
        assert_eq!(form.validate(), Err(BadForm::Email));
    }
    #[test]
    pub fn bad_email_3() {
        let form = CommentForm {
            email: "@kalle".into(),
            ..Default::default()
        };
        assert_eq!(form.validate(), Err(BadForm::Email));
    }

    #[test]
    pub fn good_url() {
        let form = CommentForm {
            url: Some("https://rasmus.krats.se/".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate().unwrap().url,
            Some("https://rasmus.krats.se/".into())
        );
    }

    #[test]
    pub fn good_ws_url() {
        let form = CommentForm {
            url: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(form.validate().unwrap().url, None);
    }

    #[test]
    pub fn good_url_nonexistent_tld() {
        let form = CommentForm {
            url: Some("https://name.blurgel/".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate().unwrap().url,
            Some("https://name.blurgel/".into())
        );
    }
    #[test]
    pub fn good_url_punycode() {
        let form = CommentForm {
            url: Some("https://☃.se/".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate().unwrap().url,
            Some("https://xn--n3h.se/".into())
        );
    }

    #[test]
    pub fn bad_url_1() {
        let form = CommentForm {
            url: Some("ftp://hello".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate(),
            Err(BadForm::Url("Must be https or http.".into()))
        );
    }

    #[test]
    pub fn bad_url_2() {
        let form = CommentForm {
            url: Some("https://he ll.o/??!".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate(),
            Err(BadForm::Url("invalid international domain name".into()))
        );
    }

    #[test]
    pub fn bad_url_3() {
        let form = CommentForm {
            url: Some("https://\01hello.se/??!".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate(),
            Err(BadForm::Url("invalid international domain name".into()))
        );
    }
    #[test]
    pub fn bad_url_blacklist() {
        let form = CommentForm {
            url: Some("https://short-url.org/1qZkf".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate(),
            Err(BadForm::Url("Please use a non-shortened url".into()))
        );
    }
    #[test]
    pub fn bad_url_punycode() {
        let form = CommentForm {
            url: Some("https://xn--aa.se/".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate(),
            Err(BadForm::Url("invalid international domain name".into()))
        );
    }

    #[test]
    pub fn normalzied_url() {
        let form = CommentForm {
            url: Some("https:///hello".into()),
            ..Default::default()
        };
        assert_eq!(
            form.validate().unwrap().url,
            Some("https://hello/".into())
        );
    }
}
