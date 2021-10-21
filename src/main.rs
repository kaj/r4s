//! An example web service using ructe with the warp framework.
#![forbid(unsafe_code)]
#[macro_use]
extern crate diesel;

mod dbopt;
mod listposts;
mod models;
mod readfiles;
mod schema;
mod server;

use anyhow::{Context, Result};
use dotenv::dotenv;
use structopt::StructOpt;

/// Main program: Set up env and run according to arguments.
#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<()> {
    match dotenv() {
        Ok(_) => (),
        Err(ref err) if err.not_found() => (),
        Err(e) => return Err(e).context("Failed to read .env"),
    }
    env_logger::init();
    R4s::from_args().run().await
}

/// Manage and serve my blog
#[derive(StructOpt)]
#[structopt(about, author)]
enum R4s {
    /// List known posts
    List(listposts::Args),
    /// Read content from markdown files
    ReadFiles(readfiles::Args),
    /// Run the web server
    RunServer(server::Args),
}

impl R4s {
    async fn run(self) -> Result<()> {
        match self {
            R4s::List(args) => args.run(),
            R4s::ReadFiles(args) => args.run(),
            R4s::RunServer(args) => args.run().await,
        }
    }
}
