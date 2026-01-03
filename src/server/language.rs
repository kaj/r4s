use super::Result;
use super::error::ViewError;
use crate::models::MyLang;
use i18n_embed::LanguageLoader;
use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::info;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

static MYLANGS: [MyLang; 2] = [MyLang::En, MyLang::Sv];

#[tracing::instrument]
fn load(lang: &str) -> Result<FluentLanguageLoader> {
    let start = Instant::now();
    let lang = lang.parse().map_err(|e| {
        tracing::error!("Bad language: {}", e);
        ViewError::BadRequest("Bad language".into())
    })?;
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_languages(
            &Localizations,
            &[lang, loader.fallback_language().clone()],
        )
        .map_err(|e| {
            tracing::error!("Missing language: {}", e);
            ViewError::BadRequest("Unknown language".into())
        })?;
    loader.set_use_isolating(false);
    info!("Loaded lang in {:?}", start.elapsed());
    Ok(loader)
}

static SV: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| load(MyLang::Sv.as_ref()).unwrap());
static EN: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| load(MyLang::En.as_ref()).unwrap());

impl MyLang {
    #[tracing::instrument]
    pub fn fluent(&self) -> &'static FluentLanguageLoader {
        match self {
            MyLang::En => &*EN,
            MyLang::Sv => &*SV,
        }
    }
    pub fn other(
        &self,
        fmt: impl Fn(&FluentLanguageLoader, &str, &str) -> String,
    ) -> Vec<String> {
        MYLANGS
            .iter()
            .filter(|&lang| lang != self)
            .map(|lang| {
                let fluent = lang.fluent();
                let name = fl!(fluent, "lang-name");
                fmt(fluent, lang.as_ref(), &name)
            })
            .collect()
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
                .unwrap_or_default(),
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
