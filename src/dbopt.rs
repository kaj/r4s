use anyhow::Result;
pub use deadpool_diesel::postgres::{Connection, Pool, PoolError};
use deadpool_diesel::{postgres::Manager, Runtime};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::Connection as _;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct DbOpt {
    /// How to connect to the postgres database.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,
}

impl DbOpt {
    /// Get a single database connection from the configured url.
    pub fn get_db(&self) -> Result<PgConnection, ConnectionError> {
        PgConnection::establish(&self.db_url)
    }

    /// Get a database connection pool from the configured url.
    pub fn build_pool(&self) -> Result<Pool> {
        Ok(Pool::builder(Manager::new(&self.db_url, Runtime::Tokio1))
            .build()?)
    }
}
