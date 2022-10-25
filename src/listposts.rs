use crate::dbopt::DbOpt;
use crate::models::PostLink;
use anyhow::Result;
use clap::Parser;
use diesel::prelude::*;

#[derive(Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// The paths to read content from.
    #[clap(long, short = 'b', default_value = "")]
    public_base: String,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let mut db = self.db.get_db()?;
        let posts = PostLink::select().load::<PostLink>(&mut db)?;
        for post in posts {
            println!(
                "{:4}. {}{:18} {}",
                post.id,
                self.public_base,
                post.url(),
                post.title
            );
        }
        Ok(())
    }
}
