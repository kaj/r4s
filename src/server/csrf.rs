use super::{Result, ViewError, ViewResult};
use base64::prelude::*;
use csrf::{AesGcmCsrfProtection, CsrfCookie, CsrfProtection, CsrfToken};
use std::str::FromStr;

#[derive(Clone)]
pub struct Secret {
    secret: [u8; 32],
}
impl FromStr for Secret {
    type Err = BadLengthSecret;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Secret {
            secret: s
                .as_bytes()
                .try_into()
                .map_err(|_| BadLengthSecret(s.len()))?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Bad CSRF secret, got {0} bytes, expected 32")]
pub struct BadLengthSecret(usize);

/// Server data for csrf handling.
pub struct Server {
    prot: AesGcmCsrfProtection,
}
impl Server {
    pub fn from_key(key: &Secret) -> Self {
        Self {
            prot: AesGcmCsrfProtection::from_key(key.secret),
        }
    }
    pub fn verify(&self, token: &str, cookie: &str) -> Result<()> {
        fn fail<E: std::fmt::Display>(e: E) -> ViewError {
            tracing::info!("Csrf verification error: {}", e);
            ViewError::BadRequest("CSRF Verification Failed".into())
        }
        let token = BASE64_STANDARD.decode(token).map_err(fail)?;
        let cookie = BASE64_STANDARD.decode(cookie).map_err(fail)?;
        let token = self.prot.parse_token(&token).map_err(fail)?;
        let cookie = self.prot.parse_cookie(&cookie).map_err(fail)?;
        self.prot.verify_token_pair(&token, &cookie).map_err(fail)
    }
    pub fn generate_pair(&self) -> Result<(CsrfToken, CsrfCookie)> {
        let ttl = 4 * 3600;
        self.prot.generate_token_pair(None, ttl).or_ise()
    }
}
