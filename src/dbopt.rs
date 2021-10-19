//use deadpool_diesel::postgres::{Manager, Pool};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use structopt::StructOpt;

//pub type PgPool = Pool;

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

    /*    /// Get a database connection pool from the configured url.
    pub fn get_pool(&self) -> PgPool {
        Pool::new(Manager::new(&self.db_url), 8)
    } */
}
