pub(crate) mod boundary;
mod identity;
mod publish;
mod read;
pub(crate) mod stream;

use crate::{config::Config, error::Error};
use sqlx::{Connection, PgConnection, PgPool, postgres::PgPoolOptions};

mod model;
pub(crate) use model::*;

#[cfg(test)]
mod tests;

const GLOBAL_LOCK_DOMAIN: i32 = 0;
const IDENTITY_LOCK: i32 = 1;
const ALLOCATION_BARRIER: i32 = 2;

#[derive(Clone)]
/// PostgreSQL access for durable backend state.
///
/// `primary` serves writes and validation reads. `read` is the configured
/// replica when one exists, or the primary pool otherwise.
pub struct Store {
    pub primary: PgPool,
    pub read: PgPool,
}

impl Store {
    /// Read the authoritative clock used to form finite expiry timestamps.
    pub(crate) async fn clock_ns(&self) -> Result<i64, Error> {
        Ok(sqlx::query_scalar!(
            r#"SELECT (extract(epoch FROM clock_timestamp()) * 1000000000)::bigint AS "now!""#
        )
        .fetch_one(&self.primary)
        .await?)
    }

    /// Connect to the primary, apply migrations, and configure the read pool.
    ///
    /// A replica is never used for migrations. If no replica URL is configured,
    /// both fields share the primary pool and all reads use the same database.
    pub async fn connect(config: &Config) -> Result<Self, Error> {
        let primary = connect_pool(&config.database.url, config).await?;
        sqlx::migrate!().run(&primary).await?;
        let read = match &config.database.replica_url {
            Some(url) => connect_pool(url, config).await?,
            None => primary.clone(),
        };
        Ok(Self { primary, read })
    }
}

/// Create a pool whose connections enforce the configured PostgreSQL statement timeout.
///
/// The timeout is installed in `after_connect`, so every connection in the pool
/// has the same bound, including connections returned after pool replacement.
async fn connect_pool(url: &str, config: &Config) -> Result<PgPool, Error> {
    let timeout = format!("{}ms", config.database.max_statement_timeout_ms);
    Ok(PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .after_connect(move |connection, _| {
            let timeout = timeout.clone();
            Box::pin(async move { configure(connection, &timeout).await })
        })
        .connect(url)
        .await?)
}

/// Open the tailer's dedicated selected-read connection outside the request pool.
/// Its loss must remain visible to recovery, even when the pool replaces connections.
/// The same TLS options and statement timeout apply to both connection paths.
pub(crate) async fn dedicated_read(pool: &PgPool, timeout_ms: u64) -> Result<PgConnection, Error> {
    let mut connection = PgConnection::connect_with(&pool.connect_options()).await?;
    configure(&mut connection, &format!("{timeout_ms}ms")).await?;
    Ok(connection)
}

async fn configure(connection: &mut PgConnection, timeout: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("SELECT set_config('statement_timeout', $1, false)", timeout)
        .fetch_one(connection)
        .await?;
    Ok(())
}
