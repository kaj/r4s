use super::{DateTime, MyLang, Slug, Tag};
use crate::schema::posts;
use i18n_embed_fl::fl;

#[derive(Debug, Queryable, Identifiable)]
pub struct Post {
    pub id: i32,
    pub slug: Slug,
    pub lang: MyLang,
    pub title: String,
    pub posted_at: DateTime,
    pub updated_at: DateTime,
    pub content: String,
}

impl Post {
    pub fn url(&self) -> String {
        format!("/{}/{}.{}", self.year(), self.slug, self.lang)
    }
    pub fn year(&self) -> i16 {
        self.posted_at.year()
    }
    pub fn publine(&self, tags: &[Tag]) -> String {
        let lang = self.lang.fluent();
        let mut line = fl!(lang, "posted-at", date = (&self.posted_at));

        if self.updated_at > self.posted_at {
            line.push(' ');
            line.push_str(&fl!(
                lang,
                "updated-at",
                date = (&self.updated_at)
            ));
        }
        fn push_taglink(to: &mut String, tag: &Tag, lang: MyLang) {
            to.push_str(" <a href='/tag/");
            to.push_str(&tag.slug);
            to.push('.');
            to.push_str(lang.as_ref());
            to.push_str("' rel='tag'>");
            to.push_str(&tag.name);
            to.push_str("</a>");
        }
        if let Some((first, rest)) = tags.split_first() {
            line.push(' ');
            line.push_str(&fl!(lang, "tagged"));
            push_taglink(&mut line, first, self.lang);
            for tag in rest {
                line.push(',');
                push_taglink(&mut line, tag, self.lang);
            }
            line.push('.');
        }
        line
    }
}
