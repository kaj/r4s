use super::error::{ViewError, ViewResult};
use super::Result;
use accept_language::intersection;
use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::LanguageLoader;
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use rust_icu_ucol::UCollator;
use std::fmt::{self, Display};
use std::str::FromStr;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

#[tracing::instrument]
pub fn load(lang: &str) -> Result<FluentLanguageLoader> {
    let lang = lang.parse().map_err(|e| {
        tracing::error!("Bad language: {}", e);
        ViewError::BadRequest("bad language".into())
    })?;
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_languages(&Localizations, &[&lang, loader.fallback_language()])
        .map_err(|e| {
            tracing::error!("Missing language: {}", e);
            ViewError::BadRequest("unknown language".into())
        })?;
    loader.set_use_isolating(false);
    Ok(loader)
}

/// Either "sv" or "en".
#[derive(Clone, Debug)]
pub struct MyLang(String);

impl MyLang {
    #[tracing::instrument]
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
    pub fn collator(&self) -> Result<UCollator> {
        UCollator::try_from(self.0.as_str()).ise()
    }
}
impl FromStr for MyLang {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, ()> {
        ["en", "sv"]
            .iter()
            .find(|&l| *l == value)
            .map(|&l| MyLang(l.into()))
            .ok_or(())
    }
}
impl Default for MyLang {
    fn default() -> Self {
        MyLang("en".into())
    }
}
impl AsRef<str> for MyLang {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl Display for MyLang {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(out)
    }
}

/// Wrapper type to get a MyLang from an accept-language header.
pub struct AcceptLang(MyLang);
impl AcceptLang {
    pub fn lang(self) -> MyLang {
        self.0
    }
}

impl FromStr for AcceptLang {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, ()> {
        Ok(AcceptLang(MyLang(
            intersection(value, vec!["en", "sv"])
                .drain(..)
                .next()
                .ok_or(())?,
        )))
    }
}
