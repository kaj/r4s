pub use i18n_embed::fluent::FluentLanguageLoader;
pub use i18n_embed::LanguageLoader;
pub use i18n_embed_fl::fl;
use reqwest::Url;

pub fn tweet_url(text: &str, url: &str) -> Url {
    Url::parse_with_params(
        "https://twitter.com/share",
        &[("text", text), ("url", url), ("via", "rasmus_kaj")]
    ).unwrap()
}
pub fn fb_share_url(url: &str) -> Url {
    Url::parse_with_params(
        "https://www.facebook.com/sharer/sharer.php",
        &[("u", url), ("display", "popup")]
    ).unwrap()
}
