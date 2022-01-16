use super::error::{ViewError, ViewResult};
use super::Result;
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

static MYLANGS: [MyLang; 2] = [MyLang::En, MyLang::Sv];

#[tracing::instrument]
pub fn load(lang: &str) -> Result<FluentLanguageLoader> {
    let lang = lang.parse().map_err(|e| {
        tracing::error!("Bad language: {}", e);
        ViewError::BadRequest("Bad language".into())
    })?;
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_languages(&Localizations, &[&lang, loader.fallback_language()])
        .map_err(|e| {
            tracing::error!("Missing language: {}", e);
            ViewError::BadRequest("Unknown language".into())
        })?;
    loader.set_use_isolating(false);
    Ok(loader)
}

/// Either "sv" or "en".
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MyLang {
    En,
    Sv,
}

impl MyLang {
    #[tracing::instrument]
    pub fn fluent(&self) -> Result<FluentLanguageLoader> {
        load(self.as_ref())
    }
    pub fn other(
        &self,
        fmt: impl Fn(FluentLanguageLoader, &str, &str) -> String,
    ) -> Vec<String> {
        MYLANGS
            .iter()
            .filter(|&lang| lang != self)
            .map(|lang| {
                let fluent = lang.fluent().unwrap();
                let name = fl!(fluent, "lang-name");
                fmt(fluent, lang.as_ref(), &name)
            })
            .collect()
    }
    pub fn collator(&self) -> Result<UCollator> {
        UCollator::try_from(self.as_ref()).ise()
    }
}
impl FromStr for MyLang {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, ()> {
        match value {
            "en" => Ok(MyLang::En),
            "sv" => Ok(MyLang::Sv),
            _ => Err(())
        }
    }
}
impl Default for MyLang {
    fn default() -> Self {
        MyLang::En
    }
}
impl AsRef<str> for MyLang {
    fn as_ref(&self) -> &str {
        match self {
            MyLang::En => "en",
            MyLang::Sv => "sv",
        }
    }
}
impl Display for MyLang {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(out)
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
        Ok(AcceptLang(
            accept_language::parse(value)
                .drain(..)
                .map(|lang| {
                    lang.split_once('-')
                        .map(|(l, _r)| l.to_string())
                        .unwrap_or(lang)
                })
                .find_map(|s| MyLang::from_str(&s).ok())
                .unwrap_or_default()
        ))
    }
}

#[test]
fn accept_1() {
    assert_eq!(
        "sv,en;q=0.7,en-US;q=0.3"
            .parse::<AcceptLang>()
            .unwrap()
            .lang(),
        MyLang::Sv,
    );
}
#[test]
fn accept_2() {
    assert_eq!(
        "fi,en;q=0.7,en-US;q=0.3"
            .parse::<AcceptLang>()
            .unwrap()
            .lang(),
        MyLang::En,
    );
}
#[test]
fn accept_3() {
    assert_eq!(
        "sv-SE".parse::<AcceptLang>().unwrap().lang(),
        "sv".parse().unwrap(),
    );
}
#[test]
fn accept_4() {
    assert_eq!(
        "en-GB".parse::<AcceptLang>().unwrap().lang(),
        "en".parse().unwrap(),
    );
}
#[test]
fn accept_5() {
    assert_eq!(
        "fi-FI".parse::<AcceptLang>().unwrap().lang(),
        "en".parse().unwrap(),
    );
}
