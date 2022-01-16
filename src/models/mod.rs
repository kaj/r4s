use diesel::sql_types::{Bool, Smallint, Timestamptz, Varchar};

mod comment;
mod datetime;
mod fullpost;
mod markdown;
mod post;
mod postlink;
mod slug;
mod tag;
mod teaser;

pub use self::comment::{Comment, PostComment};
pub use self::datetime::DateTime;
pub use self::fullpost::FullPost;
pub use self::markdown::safe_md2html;
pub use self::post::Post;
pub use self::postlink::PostLink;
pub use self::slug::Slug;
pub use self::tag::Tag;
pub use self::teaser::Teaser;

type Result<T, E = diesel::result::Error> = std::result::Result<T, E>;

sql_function! {
    fn year_of_date(arg: Timestamptz) -> Smallint;
}

sql_function! {
    fn has_lang(yearp: Smallint, slugp: Varchar, langp: Varchar) -> Bool;
}
