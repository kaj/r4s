use anyhow::{anyhow, Result};
use reqwest::blocking::{Client, Response};
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
            "<a href='{}'><img src='{}' alt='{}' width='{}' height='{}'></a>",
            self.medium.url,
            self.small.url,
            alt,
            self.small.width,
            self.small.height,
        )
    }

    pub fn markup_large(&self, alt: &str) -> String {
        format!(
            "<img src='{}' alt='{}' width='{}' height='{}'>",
            self.medium.url, alt, self.medium.width, self.medium.height,
        )
    }
    fn relative(self, base: &str) -> Self {
        ImageInfo {
            small: self.small.relative(base),
            medium: self.medium.relative(base),
            public: self.public,
        }
    }
}

pub struct ImgClient {
    web: Client,
    base: String,
    key: String,
}

impl ImgClient {
    pub fn login(
        web: Client,
        base: &str,
        user: &str,
        password: &str,
    ) -> Result<ImgClient> {
        Self::do_login(web, base, user, password).map_err(|e| {
            anyhow!("Failed to login to {:?} as {:?}: {}", base, user, e)
        })
    }
    fn do_login(
        web: Client,
        base: &str,
        user: &str,
        password: &str,
    ) -> Result<ImgClient> {
        let base = String::from(base);
        tracing::info!("Logging in to {:?}.", base);
        let response = web
            .post(format!("{base}/api/login"))
            .json(&BTreeMap::from([("user", user), ("password", password)]))
            .send()?;
        #[derive(Deserialize)]
        struct R {
            token: String,
        }
        let key = check(response)?.json::<R>()?.token;
        Ok(ImgClient { web, base, key })
    }
    pub fn fetch_image(&self, imgref: &str) -> Result<ImageInfo> {
        let response = self
            .web
            .get(format!("{}/api/image", self.base))
            .header("authorization", &self.key)
            .query(&[("path", imgref)])
            .send()?;
        Ok(check(response)?.json::<ImageInfo>()?.relative(&self.base))
    }
    pub fn make_image_public(&self, imgref: &str) -> Result<ImageInfo> {
        let response = self
            .web
            .post(format!("{}/api/image/makepublic", self.base))
            .header("authorization", &self.key)
            .json(&BTreeMap::from([("path", imgref)]))
            .send()?;
        Ok(check(response)?.json::<ImageInfo>()?.relative(&self.base))
    }
}
fn check(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let err: ImgErr = response.json()?;
        Err(anyhow!("{}: {}", status, err.err))
    }
}

#[derive(Debug, Deserialize)]
struct ImgLink {
    url: String,
    width: u32,
    height: u32,
}

impl ImgLink {
    fn relative(mut self, base: &str) -> Self {
        self.url = format!("{}{}", base, self.url);
        self
    }
}

#[derive(Debug, Deserialize)]
struct ImgErr {
    err: String,
}
