use anyhow::Result;
use clap::Parser;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::Connection as _;
use diesel_async::pooled_connection::deadpool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;

/// An asynchronous postgres database connection pool.
pub type Pool = deadpool::Pool<AsyncPgConnection>;
pub type Connection = deadpool::Object<AsyncPgConnection>;

#[derive(Parser)]
pub struct DbOpt {
    /// How to connect to the postgres database.
    #[clap(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,
}

impl DbOpt {
    /// Get a single database connection from the configured url.
    ///
    /// Since this is for one-of admin tasks, it is an ordinary synchronous connection.
    #[tracing::instrument(skip(self), err)]
    pub fn get_db(&self) -> Result<PgConnection, ConnectionError> {
        PgConnection::establish(&self.db_url)
    }

    /// Get a database connection pool from the configured url.
    ///
    /// Since this is mainly for the web server, the pooled connections are async.
    pub fn build_pool(&self) -> Result<Pool> {
        let config = AsyncDieselConnectionManager::new(&self.db_url);
        Ok(Pool::builder(config).max_size(20).build()?)
    }
}
