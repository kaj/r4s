use super::{year_of_date, DateTime, PostLink, Result};
use crate::schema::comments::dsl as c;
use crate::schema::posts::dsl as p;
use crate::server::ToHtml;
use diesel::dsl::sql;
use diesel::pg::PgConnection;
use diesel::prelude::*;

#[derive(Debug, Queryable)]
pub struct Comment {
    pub id: i32,
    pub posted_at: DateTime,
    pub content: String,
    pub name: String,
    pub email: String,
    pub url: Option<String>,
}

impl Comment {
    pub fn for_post(post_id: i32, db: &PgConnection) -> Result<Vec<Comment>> {
        c::comments
            .select((
                c::id,
                c::posted_at,
                c::content,
                c::name,
                c::email,
                c::url,
            ))
            .filter(c::post_id.eq(post_id))
            .filter(c::is_public.eq(true))
            .order_by(c::posted_at.asc())
            .load(db)
    }

    pub fn html_id(&self) -> String {
        format!("c{:x}", self.id)
    }
    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn gravatar(&self) -> String {
        use gravatar::{Default, Gravatar, Rating};
        Gravatar::new(&self.email)
            .set_size(Some(160))
            .set_default(Some(Default::Retro))
            .set_rating(Some(Rating::Pg))
            .image_url()
            .to_string()
    }
    /// Get a thing that implemnts ToHtml displaying the poster
    /// name of this comment, linked to the url if there is an url.
    pub fn link_name(&self) -> LinkName {
        LinkName(self)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn posted_at(&self) -> &DateTime {
        &self.posted_at
    }
}

pub struct LinkName<'a>(&'a Comment);

impl<'a> ToHtml for LinkName<'a> {
    fn to_html(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        if let Some(url) = &self.0.url {
            write!(out, "<a href='")?;
            url.to_html(out)?;
            write!(out, "' rel='author noopener nofollow'>")?;
            self.0.name.to_html(out)?;
            write!(out, "</a>")
        } else {
            self.0.name.to_html(out)
        }
    }
}

#[derive(Debug, Queryable)]
pub struct PostComment {
    comment: Comment,
    post: PostLink,
}

impl PostComment {
    pub fn recent(db: &PgConnection) -> Result<Vec<PostComment>> {
        c::comments
            .inner_join(p::posts.on(p::id.eq(c::post_id)))
            .select((
                (c::id, c::posted_at, c::raw_md, c::name, c::email, c::url),
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                ),
            ))
            .filter(c::is_public.eq(true))
            .filter(sql("now() - comments.posted_at < '10 weeks'"))
            .order_by(c::posted_at.desc())
            .limit(5)
            .load(db)
    }

    pub fn mod_queue(db: &PgConnection) -> Result<Vec<PostComment>> {
        c::comments
            .inner_join(p::posts.on(p::id.eq(c::post_id)))
            .select((
                (c::id, c::posted_at, c::raw_md, c::name, c::email, c::url),
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                ),
            ))
            .filter(c::is_public.eq(false))
            .filter(c::is_spam.eq(false))
            .order_by(c::posted_at.desc())
            .limit(50)
            .load(db)
    }

    pub fn p(&self) -> &PostLink {
        &self.post
    }
    pub fn url(&self) -> String {
        format!("{}#{}", self.post.url(), self.comment.html_id())
    }
    pub fn post_title(&self) -> &str {
        &self.post.title
    }
    pub fn text_start(&self) -> String {
        // Note: content here is the raw markdown.
        // maybe it should be "rendered" here, and if so, the short version
        // should probably be pre-baked, like the teaser for a post.
        let text = &self.comment.content;
        if text.len() < 100 {
            text.to_string()
        } else {
            let mut end = 90;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let end = text[..end].rfind(' ').unwrap_or(end);
            format!("{} …", &text[..end])
        }
    }
}
impl std::ops::Deref for PostComment {
    type Target = Comment;
    fn deref(&self) -> &Comment {
        &self.comment
    }
}
