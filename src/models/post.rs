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
        let mut line = fl!(lang, "posted-at", date = (&self.posted_at));

        if self.updated_at > self.posted_at {
            line.push(' ');
            line.push_str(&fl!(
                lang,
                "updated-at",
                date = (&self.updated_at)
            ));
        }
        fn push_taglink(to: &mut String, tag: &Tag, lang: &str) {
            to.push_str(" <a href='/tag/");
            to.push_str(&tag.slug);
            to.push('.');
            to.push_str(lang);
            to.push_str("' rel='tag'>");
            to.push_str(&tag.name);
            to.push_str("</a>");
        }
        if let Some((first, rest)) = tags.split_first() {
            line.push(' ');
            line.push_str(&fl!(lang, "tagged"));
            push_taglink(&mut line, first, &self.lang);
            for tag in rest {
                line.push(',');
                push_taglink(&mut line, tag, &self.lang);
            }
            line.push('.');
        }
        line
    }
}
