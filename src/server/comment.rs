use super::error::{ViewError, ViewResult};
use super::{wrap, App, Result};
use crate::models::PostLink;
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use diesel::dsl::sql;
use diesel::prelude::*;
use pulldown_cmark::{html::push_html, Event, Parser, Tag};
use serde::Deserialize;
use warp::filters::{cookie, BoxedFilter};
use warp::path::end;
use warp::{self, body, post, Filter, Reply};

pub fn route(s: BoxedFilter<(App,)>) -> BoxedFilter<(impl Reply,)> {
    end()
        .and(post())
        .and(cookie::cookie("CSRF"))
        .and(body::form())
        .and(s)
        .then(postcomment)
        .map(wrap)
        .boxed()
}

async fn postcomment(
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
        return Err(ViewError::BadRequest(
            "This seems like spam.  Sorry.".into(),
        ));
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
        let mut hdiff = 0;
        let markdown = Parser::new(&self.comment).map(|e| match e {
            Event::Html(s) => Event::Text(s),
            Event::Start(Tag::Heading(h)) => {
                hdiff = std::cmp::max(hdiff, 4 - h);
                Event::Start(Tag::Heading(h + hdiff))
            }
            Event::End(Tag::Heading(h)) => {
                Event::End(Tag::Heading(h + hdiff))
            }
            e => e,
        });
        let mut html = String::new();
        push_html(&mut html, markdown);
        html
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn markdown_no_html() {
        let form = super::CommentForm {
            post: 185,
            comment: "Hej!\
                      \r\n\r\nHär är <em>en</em> _kommentar_.\
                      \r\n\r\n<script>evil</script>"
                .into(),
            name: "Rasmus".into(),
            email: "rasmus@krats.se".into(),
            url: None,
        };
        assert_eq!(
            &form.html(),
            "<p>Hej!</p>\
             \n<p>Här är &lt;em&gt;en&lt;/em&gt; <em>kommentar</em>.</p>\
             \n&lt;script&gt;evil&lt;/script&gt;",
        );
    }

    #[test]
    fn heading_level() {
        let form = super::CommentForm {
            post: 185,
            comment: "# Rubrik\
                      \r\n\r\nRubriken ska hamna på rätt nivå.\
                      \r\n\r\n## Underrubrik
                      \r\n\r\nOch underrubriken på nivån under."
                .into(),
            name: "Rasmus".into(),
            email: "rasmus@krats.se".into(),
            url: None,
        };
        assert_eq!(
            &form.html(),
            "<h4>Rubrik</h4>\
             \n<p>Rubriken ska hamna på rätt nivå.</p>\
             \n<h5>Underrubrik</h5>\
             \n<p>Och underrubriken på nivån under.</p>\n",
        );
    }
    #[test]
    fn heading_level_2() {
        let form = super::CommentForm {
            post: 185,
            comment: "### Rubrik\
                      \r\n\r\nRubriken ska hamna på rätt nivå.\
                      \r\n\r\n#### Underrubrik
                      \r\n\r\nOch underrubriken på nivån under."
                .into(),
            name: "Rasmus".into(),
            email: "rasmus@krats.se".into(),
            url: None,
        };
        assert_eq!(
            &form.html(),
            "<h4>Rubrik</h4>\
             \n<p>Rubriken ska hamna på rätt nivå.</p>\
             \n<h5>Underrubrik</h5>\
             \n<p>Och underrubriken på nivån under.</p>\n",
        );
    }
}
