use anyhow::{anyhow, Result};
use reqwest::{Client, Response};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct ImageInfo {
    small: ImgLink,
    medium: ImgLink,
    public: bool,
}

impl ImageInfo {
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

pub struct ImgClient {
    base: String,
    key: String,
}

impl ImgClient {
    pub async fn login(
        base: &str,
        user: &str,
        password: &str,
    ) -> Result<ImgClient> {
        Self::do_login(base, user, password).await.map_err(|e| {
            anyhow!("Failed to login to {:?} as {:?}: {}", base, user, e)
        })
    }
    pub async fn do_login(
        base: &str,
        user: &str,
        password: &str,
    ) -> Result<ImgClient> {
        let base = String::from(base);
        tracing::info!("Logging in to {:?}.", base);
        let response = Client::new()
            .post(&format!("{}/api/login", base))
            .json(&BTreeMap::from([("user", user), ("password", password)]))
            .send()
            .await?;
        #[derive(Deserialize)]
        struct R {
            token: String,
        }
        let key = check(response).await?.json::<R>().await?.token;
        Ok(ImgClient { base, key })
    }
    pub async fn fetch_image(&self, imgref: &str) -> Result<ImageInfo> {
        let response = Client::new()
            .get(&format!("{}/api/image", self.base))
            .header("authorization", &self.key)
            .query(&[("path", imgref)])
            .send()
            .await?;
        Ok(check(response).await?.json().await?)
    }
    pub async fn make_image_public(&self, imgref: &str) -> Result<ImageInfo> {
        let response = Client::new()
            .post(&format!("{}/api/image/makepublic", self.base))
            .header("authorization", &self.key)
            .json(&BTreeMap::from([("path", imgref)]))
            .send()
            .await?;
        Ok(check(response).await?.json().await?)
    }
}
async fn check(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let err: ImgErr = response.json().await?;
        Err(anyhow!("{}: {}", status, err.err))
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
