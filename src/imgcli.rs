use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ImageInfo {
    small: ImgLink,
    medium: ImgLink,
    public: bool,
}

impl ImageInfo {
    pub async fn fetch(imgref: &str) -> Result<ImageInfo> {
        Ok(Client::new()
            .get("https://img.krats.se/api/image")
            .query(&[("path", imgref)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub fn markup(&self, alt: &str) -> String {
        format!(
            "<a href='https://img.krats.se{}'><img src='https://img.krats.se{}' \
             alt='{}' width='{}' height='{}'></a>",
            self.medium.url,
            self.small.url,
            alt,
            self.small.width,
            self.small.height,
        )
    }

    pub fn markup_large(&self, alt: &str) -> String {
        format!(
            "<img src='https://img.krats.se{}' alt='{}' width='{}' height='{}'>",
            self.medium.url,
            alt,
            self.medium.width,
            self.medium.height,
        )
    }
}

#[derive(Debug, Deserialize)]
struct ImgLink {
    url: String,
    width: u32,
    height: u32,
}
