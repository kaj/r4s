use super::{Result, ViewError};
use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::LanguageLoader;
use rust_embed::RustEmbed;

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
