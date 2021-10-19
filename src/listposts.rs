use crate::dbopt::DbOpt;
use crate::models::PostLink;
use anyhow::Result;
use diesel::prelude::*;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// The paths to read content from.
    #[structopt(long, short = "b", default_value = "")]
    public_base: String,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        let posts = PostLink::select().load::<PostLink>(&db)?;
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
