mod identity;
mod publish;
mod read;

use crate::{config::Config, error::Error};
use sqlx::{PgPool, postgres::PgPoolOptions};

mod model;
pub(crate) use model::*;

#[derive(Clone)]
pub struct Store {
    pub primary: PgPool,
    pub read: PgPool,
}

impl Store {
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
