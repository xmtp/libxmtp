mod identity;
mod publish;
mod read;

use crate::{config::Config, error::Error};
use sqlx::{PgPool, postgres::PgPoolOptions};

mod model;
pub(crate) use model::*;

#[cfg(test)]
mod tests;

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
            Box::pin(async move {
                sqlx::query!("SELECT set_config('statement_timeout', $1, false)", timeout)
                    .fetch_one(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await?)
}
