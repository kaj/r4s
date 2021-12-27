use crate::schema::comments::dsl as c;
use crate::schema::post_tags::dsl as pt;
use crate::schema::posts::dsl as p;
use crate::schema::tags::dsl as t;
use diesel::dsl::{not, sql};
use diesel::helper_types::Select;
use diesel::pg::{Pg, PgConnection};
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Bool, Smallint, Timestamptz, Varchar};
use fluent::types::FluentType;
use fluent::FluentValue;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed_fl::fl;
use intl_memoizer::concurrent::IntlLangMemoizer as CcIntlLangMemoizer;
use intl_memoizer::IntlLangMemoizer;
use std::borrow::Cow;

sql_function! {
    fn year_of_date(arg: Timestamptz) -> Smallint;
}

sql_function! {
    fn has_lang(yearp: Smallint, slugp: Varchar, langp: Varchar) -> Bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
pub struct DateTime(chrono::DateTime<chrono::Utc>);

impl DateTime {
    pub fn wrap(v: chrono::DateTime<chrono::Utc>) -> Self {
        DateTime(v)
    }
    pub fn raw(&self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl Queryable<Timestamptz, Pg> for DateTime {
    type Row =
        <chrono::DateTime<chrono::Utc> as Queryable<Timestamptz, Pg>>::Row;
    fn build(row: Self::Row) -> Self {
        DateTime(chrono::DateTime::<chrono::Utc>::build(row))
    }
}

impl<'a> From<&'a DateTime> for FluentValue<'static> {
    fn from(val: &'a DateTime) -> FluentValue<'static> {
        FluentValue::Custom(val.duplicate())
    }
}

impl FluentType for DateTime {
    fn duplicate(&self) -> Box<dyn FluentType + Send + 'static> {
        Box::new(*self)
    }
    fn as_string(&self, _intls: &IntlLangMemoizer) -> Cow<'static, str> {
        self.0.format("%Y-%m-%d %H:%M").to_string().into()
    }
    fn as_string_threadsafe(
        &self,
        _intls: &CcIntlLangMemoizer,
    ) -> Cow<'static, str> {
        self.0.format("%Y-%m-%d %H:%M").to_string().into()
    }
}

#[derive(Debug, Queryable)]
pub struct PostLink {
    pub id: i32,
    pub year: i16,
    pub slug: String,
    pub lang: String,
    pub title: String,
}

impl PostLink {
    pub fn select() -> Select<
        p::posts,
        (
            p::id,
            year_of_date::year_of_date<p::posted_at>,
            p::slug,
            p::lang,
            p::title,
        ),
    > {
        p::posts.select((
            p::id,
            year_of_date(p::posted_at),
            p::slug,
            p::lang,
            p::title,
        ))
    }
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year, self.slug, self.lang)
    }
}

#[derive(Debug, Queryable)]
pub struct Post {
    pub id: i32,
    pub year: i16,
    pub slug: String,
    pub lang: String,
    pub title: String,
    pub posted_at: DateTime,
    pub updated_at: DateTime,
    pub content: String,
}

impl Post {
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year, self.slug, self.lang)
    }
    pub fn publine(
        &self,
        lang: &FluentLanguageLoader,
        tags: &[Tag],
    ) -> String {
        use std::fmt::Write;
        let mut line = fl!(lang, "posted-at", date = (&self.posted_at));

        if self.updated_at > self.posted_at {
            write!(
                &mut line,
                " {}",
                fl!(lang, "updated-at", date = (&self.updated_at))
            )
            .unwrap();
        }
        if let Some((first, rest)) = tags.split_first() {
            write!(
                line,
                " {} <a href='/tag/{slug}.{lang}' rel='tag'>{name}</a>",
                fl!(lang, "tagged"),
                slug = first.slug,
                name = first.name,
                lang = self.lang,
            )
            .unwrap();
            for tag in rest {
                write!(
                    line,
                    ", <a href='/tag/{slug}.{lang}' rel='tag'>{name}</a>",
                    slug = tag.slug,
                    name = tag.name,
                    lang = self.lang,
                )
                .unwrap();
            }
            line.push('.');
        }
        line
    }
}

pub struct Teaser {
    post: Post,
    tags: Vec<Tag>,
    /// True if the full text of the post is more than this teaser.
    is_more: bool,
    n_comments: u32,
}

impl Teaser {
    pub fn recent(
        lang: &str,
        limit: u32,
        db: &PgConnection,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(p::lang.eq(&lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                &lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.desc())
            .limit(limit.into())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn for_year(
        year: i16,
        lang: &str,
        db: &PgConnection,
    ) -> Result<Vec<Teaser>, diesel::result::Error> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(
                year_of_date(p::posted_at)
                    .eq(year)
                    .or(year_of_date(p::updated_at).eq(year)),
            )
            .filter(p::lang.eq(&lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                &lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.asc())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn tagged(
        tag_id: i32,
        lang: &str,
        limit: u32,
        db: &PgConnection,
    ) -> Result<Vec<Teaser>, diesel::result::Error> {
        p::posts
            .left_join(
                c::comments
                    .on(c::post_id.eq(p::id).and(c::is_public.eq(true))),
            )
            .select((
                (
                    p::id,
                    year_of_date(p::posted_at),
                    p::slug,
                    p::lang,
                    p::title,
                    p::posted_at,
                    p::updated_at,
                    p::teaser,
                ),
                p::teaser.ne(p::content),
                sql::<BigInt>("count(distinct comments.id)"),
            ))
            .filter(
                p::id.eq_any(
                    pt::post_tags
                        .select(pt::post_id)
                        .filter(pt::tag_id.eq(tag_id)),
                ),
            )
            .filter(p::lang.eq(&lang).or(not(has_lang(
                year_of_date(p::posted_at),
                p::slug,
                &lang,
            ))))
            .group_by(p::posts::all_columns())
            .order(p::updated_at.desc())
            .limit(limit.into())
            .load::<(Post, bool, i64)>(db)?
            .into_iter()
            .map(|(post, is_more, n_comments)| {
                Tag::for_post(post.id, db).map(|tags| Teaser {
                    post,
                    tags,
                    is_more,
                    n_comments: n_comments as _,
                })
            })
            .collect()
    }
    pub fn publine(&self) -> String {
        // TODO: Take the fluent as an argument instead?
        let lang = crate::server::language::load(&self.lang).unwrap();
        self.post.publine(&lang, &self.tags)
    }
    pub fn readmore(&self) -> String {
        // TODO: Take the fluent as an argument instead?
        let lang = crate::server::language::load(&self.lang).unwrap();
        match (self.is_more, self.n_comments > 0) {
            (true, true) => fl!(
                lang,
                "read-more-comments",
                title = self.title.as_str(),
                n = self.n_comments
            ),
            (true, false) => {
                fl!(lang, "read-more", title = self.title.as_str())
            }
            (false, true) => fl!(lang, "read-comments", n = self.n_comments),
            (false, false) => fl!(lang, "comment-first"),
        }
    }
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }
}

impl std::ops::Deref for Teaser {
    type Target = Post;
    fn deref(&self) -> &Post {
        &self.post
    }
}

#[derive(Debug, Queryable)]
pub struct Tag {
    pub id: i32,
    pub slug: String,
    pub name: String,
}

impl Tag {
    pub fn by_slug(
        slug: &str,
        db: &PgConnection,
    ) -> Result<Option<Tag>, diesel::result::Error> {
        t::tags.filter(t::slug.eq(slug)).first::<Tag>(db).optional()
    }
    pub fn for_post(
        post_id: i32,
        db: &PgConnection,
    ) -> Result<Vec<Tag>, diesel::result::Error> {
        t::tags
            .filter(
                t::id.eq_any(
                    pt::post_tags
                        .select(pt::tag_id)
                        .filter(pt::post_id.eq(post_id)),
                ),
            )
            .load(db)
    }
}

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
    pub fn for_post(
        post_id: i32,
        db: &PgConnection,
    ) -> Result<Vec<Comment>, diesel::result::Error> {
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
}

#[derive(Debug, Queryable)]
pub struct PostComment {
    comment: Comment,
    post: PostLink,
}

impl PostComment {
    pub fn recent(
        db: &PgConnection,
    ) -> Result<Vec<PostComment>, diesel::result::Error> {
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

    pub fn mod_queue(
        db: &PgConnection,
    ) -> Result<Vec<PostComment>, diesel::result::Error> {
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

    pub fn c(&self) -> &Comment {
        &self.comment
    }
    pub fn p(&self) -> &PostLink {
        &self.post
    }
    pub fn url(&self) -> String {
        format!("{}#{}", self.post.url(), self.comment.html_id())
    }
    pub fn gravatar(&self) -> String {
        self.comment.gravatar()
    }
    pub fn name(&self) -> &str {
        &self.comment.name
    }
    pub fn posted_at(&self) -> &DateTime {
        &self.comment.posted_at
    }
    pub fn post_title(&self) -> &str {
        &self.post.title
    }
    pub fn text_start(&self) -> String {
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

mod markdown {
    use pulldown_cmark::{html::push_html, Event, HeadingLevel, Parser, Tag};

    pub fn safe_md2html(raw: &str) -> String {
        let mut hdiff = 0;
        let markdown = Parser::new(raw).map(|e| match e {
            Event::Html(s) => Event::Text(s),
            Event::Start(Tag::Heading(h, id, cls)) => {
                hdiff = std::cmp::max(hdiff, 3 - h as i8);
                Event::Start(Tag::Heading(hl(h as i8 + hdiff), id, cls))
            }
            Event::End(Tag::Heading(h, id, cls)) => {
                Event::End(Tag::Heading(hl(h as i8 + hdiff), id, cls))
            }
            e => e,
        });
        let mut html = String::new();
        push_html(&mut html, markdown);
        html
    }

    pub fn hl(i: i8) -> HeadingLevel {
        match i {
            0 => HeadingLevel::H1,
            1 => HeadingLevel::H2,
            2 => HeadingLevel::H3,
            3 => HeadingLevel::H4,
            4 => HeadingLevel::H5,
            5 => HeadingLevel::H6,
            _ => panic!("Bad heading level: {}", i + 1),
        }
    }

    #[test]
    fn markdown_no_html() {
        assert_eq!(
            safe_md2html(
                "Hej!\
                 \r\n\r\nHär är <em>en</em> _kommentar_.\
                 \r\n\r\n<script>evil</script>"
            ),
            "<p>Hej!</p>\
             \n<p>Här är &lt;em&gt;en&lt;/em&gt; <em>kommentar</em>.</p>\
             \n&lt;script&gt;evil&lt;/script&gt;",
        );
    }

    #[test]
    fn heading_level() {
        assert_eq!(
            safe_md2html(
                "# Rubrik\
                 \r\n\r\nRubriken ska hamna på rätt nivå.\
                 \r\n\r\n## Underrubrik\
                 \r\n\r\nOch underrubriken på nivån under."
            ),
            "<h4>Rubrik</h4>\
             \n<p>Rubriken ska hamna på rätt nivå.</p>\
             \n<h5>Underrubrik</h5>\
             \n<p>Och underrubriken på nivån under.</p>\n",
        );
    }
    #[test]
    fn heading_level_2() {
        assert_eq!(
            safe_md2html(
                "### Rubrik\
                 \r\n\r\nRubriken ska hamna på rätt nivå.\
                 \r\n\r\n#### Underrubrik\
                 \r\n\r\nOch underrubriken på nivån under."
            ),
            "<h4>Rubrik</h4>\
             \n<p>Rubriken ska hamna på rätt nivå.</p>\
             \n<h5>Underrubrik</h5>\
             \n<p>Och underrubriken på nivån under.</p>\n",
        );
    }
}
pub use markdown::safe_md2html;
