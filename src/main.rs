//! An example web service using ructe with the warp framework.
#![forbid(unsafe_code)]
#[macro_use]
extern crate diesel;

mod dbopt;
mod imgcli;
mod listposts;
mod modcomments;
mod models;
mod readcomments;
mod readfiles;
mod schema;
mod server;
mod syntax_hl;

use anyhow::{Context, Result};
use dotenv::dotenv;
use structopt::StructOpt;

/// Main program: Set up env and run according to arguments.
fn main() -> Result<()> {
    match dotenv() {
        Ok(_) => (),
        Err(ref err) if err.not_found() => (),
        Err(e) => return Err(e).context("Failed to read .env"),
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").as_deref().unwrap_or("info"),
        )
        .init();

    R4s::from_args().run()
}

/// Manage and serve my blog
#[derive(StructOpt)]
#[structopt(about, author)]
enum R4s {
    /// List known posts
    List(listposts::Args),
    /// Moderate new coments
    ModerateComments(modcomments::Args),
    /// Read content from markdown files
    ReadFiles(readfiles::Args),
    /// Read comments from a json dump.
    ReadComments(readcomments::Args),
    /// Run the web server
    RunServer(server::Args),
}

impl R4s {
    fn run(self) -> Result<()> {
        match self {
            R4s::List(args) => args.run(),
            R4s::ModerateComments(args) => args.run(),
            R4s::ReadFiles(args) => args.run(),
            R4s::ReadComments(args) => run_async(args.run()),
            R4s::RunServer(args) => run_async(args.run()),
        }
    }
}

#[derive(StructOpt)]
pub struct PubBaseOpt {
    /// Base url for the server, in absolute urls
    #[structopt(long, short = "b", env = "R4S_BASE")]
    public_base: String,
}

fn run_async<F>(work: F) -> Result<()>
where
    F: std::future::Future<Output = Result<()>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(10)
        .build()?
        .block_on(work)
}
