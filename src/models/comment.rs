use super::{DateTime, Post, PostLink, Result};
use crate::dbopt::Connection;
use crate::schema::comments::{self, dsl as c};
use crate::schema::posts::dsl as p;
use crate::server::templates::{Html, HtmlBuffer, ToHtml};
use diesel::prelude::*;
use diesel::{dsl::sql, sql_types::Bool};
use diesel_async::RunQueryDsl;

#[derive(Debug, Identifiable, Queryable, Selectable, Associations)]
#[diesel(belongs_to(Post))]
pub struct Comment {
    pub id: i32,
    pub post_id: i32,
    pub posted_at: DateTime,
    pub content: String,
    pub name: String,
    pub email: String,
    pub url: Option<String>,
}

impl Comment {
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
    /// Get a thing that implemnts [`ToHtml`] displaying the poster
    /// name of this comment, linked to the url if there is an url.
    pub fn link_name(&self) -> LinkName<'_> {
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

impl ToHtml for LinkName<'_> {
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
    pub async fn recent(db: &mut Connection) -> Result<Vec<PostComment>> {
        c::comments
            .inner_join(p::posts.on(p::id.eq(c::post_id)))
            .select((Comment::as_select(), PostLink::as_select()))
            .filter(c::is_public.eq(true))
            .filter(sql::<Bool>("now() - comments.posted_at < '10 weeks'"))
            .order_by(c::posted_at.desc())
            .limit(5)
            .load(db)
            .await
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
    pub fn text_start(&self) -> HtmlBuffer {
        // Note: content here is the raw markdown.
        // maybe it should be "rendered" here, and if so, the short version
        // should probably be pre-baked, like the teaser for a post.
        let text = &self.comment.content;
        if text.len() < 200 {
            Html(text).to_buffer().unwrap()
        } else {
            let mut end = 120;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let end = text[..end].rfind(' ').unwrap_or(end);
            format!("{} …", &text[..end]).to_buffer().unwrap()
        }
    }
}
impl std::ops::Deref for PostComment {
    type Target = Comment;
    fn deref(&self) -> &Comment {
        &self.comment
    }
}
