use super::error::ViewResult;
use super::{wrap, Pool, Result, Uri};
use crate::models::PostLink;
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use diesel::prelude::*;
use pulldown_cmark::{html::push_html, Event, Parser, Tag};
use serde::Deserialize;
use warp::filters::BoxedFilter;
use warp::path::end;
use warp::redirect::found;
use warp::{self, body, post, Filter, Reply};

pub fn route(s: BoxedFilter<(Pool,)>) -> BoxedFilter<(impl Reply,)> {
    end()
        .and(post())
        .and(body::form())
        .and(s)
        .then(postcomment)
        .map(wrap)
        .boxed()
}

async fn postcomment(form: CommentForm, pool: Pool) -> Result<impl Reply> {
    let db = pool.get().await?;
    let post = form.post;
    let post = db
        .interact(move |db| {
            PostLink::select()
                .filter(p::id.eq(post))
                .first::<PostLink>(db)
        })
        .await??;
    db.interact(move |db| {
        diesel::insert_into(c::comments)
            .values((
                c::post_id.eq(&form.post),
                c::content.eq(form.html()),
                c::name.eq(&form.name),
                c::email.eq(&form.email),
                form.url.as_ref().map(|u| c::url.eq(u)),
                c::raw_md.eq(&form.comment),
            ))
            .execute(db)
    })
    .await??;

    Ok(found(
        Uri::builder().path_and_query(post.url()).build().ise()?,
    ))
}

#[derive(Debug, Deserialize)]
struct CommentForm {
    post: i32,
    comment: String,
    name: String,
    email: String,
    url: Option<String>,
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
