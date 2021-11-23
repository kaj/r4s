use super::{Result, ViewError};
use accept_language::intersection;
use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::LanguageLoader;
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use std::str::FromStr;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub fn load(lang: &str) -> Result<FluentLanguageLoader> {
    let lang = lang.parse().map_err(|e| {
        dbg!(e, lang);
        ViewError::BadRequest("bad language".into())
    })?;
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_languages(&Localizations, &[&lang, loader.fallback_language()])
        .map_err(|e| {
            dbg!(e, lang);
            ViewError::BadRequest("unknown language".into())
        })?;
    loader.set_use_isolating(false);
    Ok(loader)
}

/// Either "sv" or "en".
#[derive(Clone, Debug)]
pub struct MyLang(pub String);

impl MyLang {
    pub fn fluent(&self) -> Result<FluentLanguageLoader> {
        load(&self.0)
    }
    pub fn other(
        &self,
        fmt: impl Fn(FluentLanguageLoader, &str, &str) -> String,
    ) -> Vec<String> {
        ["sv", "en"]
            .iter()
            .filter(|&lang| lang != &self.0)
            .map(|lang| {
                let fluent = load(lang).unwrap();
                let name = fl!(fluent, "lang-name");
                fmt(fluent, lang, &name)
            })
            .collect()
    }
}

impl FromStr for MyLang {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, ()> {
        Ok(MyLang(
            intersection(value, vec!["en", "sv"])
                .drain(..)
                .next()
                .ok_or(())?,
        ))
    }
}
impl Default for MyLang {
    fn default() -> Self {
        MyLang("en".into())
    }
}
