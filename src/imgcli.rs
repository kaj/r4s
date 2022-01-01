use anyhow::{anyhow, Context, Result};
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
        Self::do_fetch(imgref).await.with_context(|| {
            format!("Query for image {:?} failed on server", imgref)
        })
    }
    async fn do_fetch(imgref: &str) -> Result<ImageInfo> {
        let response = Client::new()
            .get("https://img.krats.se/api/image")
            .query(&[("path", imgref)])
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            Ok(response.json().await?)
        } else {
            let err: ImgErr = response.json().await?;
            Err(anyhow!("{}: {}", status, err.err))
        }
    }

    pub fn is_portrait(&self) -> bool {
        self.medium.width < self.medium.height
    }

    pub fn is_public(&self) -> bool {
        self.public
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

#[derive(Debug, Deserialize)]
struct ImgErr {
    err: String,
}
