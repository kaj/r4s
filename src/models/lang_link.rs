use super::{MyLang, Slug};
use crate::server::templates::ToHtml;
use i18n_embed_fl::fl;

#[derive(Debug)]
pub struct LangLink {
    url: String,
    lang: MyLang,
    title: Option<String>,
    name: String,
}

impl LangLink {
    pub fn post(year: i16, slug: &Slug, lang: MyLang, title: String) -> Self {
        let fluent = lang.fluent();
        LangLink {
            url: format!("/{year}/{slug}.{lang}"),
            lang,
            title: Some(fl!(fluent, "in-lang", title = title)),
            name: fl!(fluent, "lang-name"),
        }
    }
    pub fn tags(lang: MyLang) -> Self {
        LangLink {
            url: format!("/tag/{lang}"),
            lang,
            title: None,
            name: fl!(lang.fluent(), "lang-name"),
        }
    }
    pub fn front(lang: MyLang) -> Self {
        LangLink {
            url: format!("/{lang}"),
            lang,
            title: None,
            name: fl!(lang.fluent(), "lang-name"),
        }
    }
    pub fn tag(tag: &str, lang: MyLang) -> Self {
        LangLink {
            url: format!("/tag/{tag}.{lang}"),
            lang,
            title: None,
            name: fl!(lang.fluent(), "lang-name"),
        }
    }
    pub fn year(year: i16, lang: MyLang) -> Self {
        LangLink {
            url: format!("/{year}/{lang}"),
            lang,
            title: None,
            name: fl!(lang.fluent(), "lang-name"),
        }
    }
    pub fn page(lang: MyLang, slug: &str, title: &str) -> Self {
        let fluent = lang.fluent();
        LangLink {
            url: format!("/{slug}.{lang}"),
            lang,
            title: Some(fl!(fluent, "in-lang", title = title)),
            name: fl!(fluent, "lang-name"),
        }
    }
}

impl ToHtml for LangLink {
    fn to_html(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        let LangLink {
            url,
            lang,
            title,
            name,
        } = self;
        write!(out, "<a href='/{url}' hreflang='{lang}' lang='{lang}'")?;
        if let Some(title) = title {
            write!(out, " title='{title}'")?;
        }
        write!(out, " rel='alternate'>{name}</a>")
        // TODO: Probably use to_html on title and name?
    }
}
