use crate::dbopt::DbOpt;
use crate::schema::avatars::dsl as a;
use crate::schema::comments::dsl as c;
use base64::prelude::*;
use bytes::Bytes;
use clap::Parser;
use diesel::{ExpressionMethods as _, JoinOnDsl as _, QueryDsl};
use diesel_async::RunQueryDsl as _;
use reqwest::{StatusCode, header::CONTENT_TYPE};
use sha2::{Digest as _, Sha256};

#[derive(Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,
}

impl Args {
    pub async fn run(self) -> anyhow::Result<()> {
        let mut db = self.db.build_pool()?.get().await?;
        let emails = c::comments
            .left_join(a::avatars.on(a::email.eq(c::email)))
            .filter(c::is_public)
            .filter(c::email.ne(""))
            .filter(a::id.is_null())
            .group_by(c::email)
            .select(c::email)
            .load::<String>(&mut db)
            .await?;

        for email in emails {
            println!("Checking {email:?} for avatar ...");
            let (ctype, data) =
                if let Some((ctype, data)) = fetch_gravatar(&email).await? {
                    (ctype, data)
                } else {
                    let data = make_animatar(&email);
                    ("image/svg+xml".to_string(), Bytes::from(data))
                };

            diesel::insert_into(a::avatars)
                .values((
                    a::email.eq(email),
                    a::mime.eq(ctype),
                    a::content.eq(data.to_vec()),
                    a::slug.eq(make_random_slug()),
                ))
                .execute(&mut db)
                .await?;

            // todo!("Save {} bytes of {ctype}", data.len());
            // todo!();
        }
        // a::avatars::select
        Ok(())
    }
}

async fn fetch_gravatar(
    email: &str,
) -> anyhow::Result<Option<(String, Bytes)>> {
    let hash = Sha256::digest(email.trim());
    let url = format!("https://gravatar.com/avatar/{hash:x}?s=160&d=404");
    let response = reqwest::get(&url).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    let ctype = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    Ok(Some((ctype, response.bytes().await?)))
}

fn make_animatar(email: &str) -> String {
    let options = animatar::AvatarOptions {
        size: 160,
        //round: todo!(),
        //blackout: todo!(),
        //avatar_colors: todo!(),
        //background_colors: todo!(),
        ..Default::default()
    };
    animatar::avatar(email.trim(), &options)
}

/*#[test]
fn test_animatar() {
    println!("{}", make_animatar("abo@kth.se"));
    todo!();
}*/

#[tokio::test]
async fn test_bad_email() {
    assert_eq!(fetch_gravatar("nonesuch@krats.se").await.unwrap(), None,);
}

#[tokio::test]
async fn test_my_email() {
    let (ctype, _data) =
        fetch_gravatar("rasmus@krats.se").await.unwrap().unwrap();
    assert_eq!(ctype, "image/jpeg",);
}

fn make_random_slug() -> String {
    // let mut rng = rand::rng();

    // 12 bytes is 96 bits is 16 chars of base64.
    let data: [u8; 12] = rand::random();

    let s = BASE64_URL_SAFE.encode(data);
    assert_eq!(s.len(), 16);
    s
}
