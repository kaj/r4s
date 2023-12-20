pub use i18n_embed::fluent::FluentLanguageLoader;
pub use i18n_embed::LanguageLoader;
pub use i18n_embed_fl::fl;
use reqwest::Url;

pub fn fb_share_url(url: &str) -> Url {
    Url::parse_with_params(
        "https://www.facebook.com/sharer/sharer.php",
        &[("u", url), ("display", "popup")],
    )
    .unwrap() // only errs if hardcoded part is bad
}
