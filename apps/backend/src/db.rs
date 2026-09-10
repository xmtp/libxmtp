pub(crate) mod boundary;
mod identity;
mod publish;
mod read;
pub(crate) mod stream;

use crate::{config::Config, error::Error};
use sqlx::{Connection, PgConnection, PgPool, Row, postgres::PgPoolOptions};

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
    #[xmtp_common::db_span]
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
    #[xmtp_common::db_span]
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
        .after_release(|connection, _| Box::pin(release(connection)))
        .connect(url)
        .await?)
}

/// Roll back transactions that SQLx did not record before pool reuse.
/// A cancelled `begin` can send BEGIN before SQLx increments its transaction
/// depth, so its drop guard queues no rollback. The pool's ping only drains
/// responses. Probe with one simple query: timestamps differ in an explicit
/// transaction. A probe error makes SQLx close an aborted connection hard.
async fn release(connection: &mut PgConnection) -> Result<bool, sqlx::Error> {
    let in_transaction: bool =
        sqlx::raw_sql("SELECT transaction_timestamp() <> statement_timestamp()")
            .fetch_one(&mut *connection)
            .await?
            .try_get(0)?;
    if in_transaction {
        crate::telemetry::released_open_transaction();
        sqlx::raw_sql("ROLLBACK").execute(connection).await?;
        tracing::warn!("rolled back an open transaction on pool release");
    }
    Ok(true)
}

/// Open the tailer's dedicated selected-read connection outside the request pool.
/// Its loss must remain visible to recovery, even when the pool replaces connections.
/// The same TLS options and statement timeout apply to both connection paths.
#[xmtp_common::db_span]
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
