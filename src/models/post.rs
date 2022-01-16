use super::{DateTime, Slug, Tag};
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed_fl::fl;

#[derive(Debug, Queryable)]
pub struct Post {
    pub id: i32,
    pub year: i16,
    pub slug: Slug,
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
