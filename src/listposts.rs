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
        for post in PostLink::all().load(&mut self.db.get_db()?)? {
            println!(
                "{:4}. {}{:20} {}",
                post.id,
                self.public_base,
                post.url(),
                post.title
            );
        }
        Ok(())
    }
}
