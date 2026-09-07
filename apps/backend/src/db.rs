mod identity;
mod publish;
mod read;

use crate::{api, config::Config, error::Error};
use prost::Message;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub(crate) use publish::PendingEnvelope;

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

#[derive(Clone, Debug)]
pub(crate) struct StoredMeta {
    pub sequence_id: i64,
    pub topic: Vec<u8>,
    pub server_ns: i64,
    pub expiry_ns: Option<i64>,
    pub message_hash: Vec<u8>,
    pub is_commit_or_proposal: bool,
}

impl StoredMeta {
    pub fn wire(self) -> api::EnvelopeMeta {
        api::EnvelopeMeta {
            cursor: Some(api::Cursor {
                sequence_id: self.sequence_id as u64,
            }),
            topic: Some(api::Topic { topic: self.topic }),
            server_ns: self.server_ns as u64,
            expiry_ns: self.expiry_ns.unwrap_or_default() as u64,
            message_hash: Some(api::MessageHash {
                hash: Some(api::message_hash::Hash::Sha256(self.message_hash)),
            }),
            is_commit_or_proposal: self.is_commit_or_proposal,
        }
    }
}

pub(crate) struct StoredEnvelope {
    pub sequence_id: i64,
    pub topic: Vec<u8>,
    pub server_ns: i64,
    pub expiry_ns: Option<i64>,
    pub message_hash: Vec<u8>,
    pub is_commit_or_proposal: bool,
    pub payload: Vec<u8>,
}

impl StoredEnvelope {
    pub fn wire(self) -> Result<api::ServerEnvelope, Error> {
        let envelope = api::ClientEnvelope::decode(self.payload.as_slice())?;
        let meta = StoredMeta {
            sequence_id: self.sequence_id,
            topic: self.topic,
            server_ns: self.server_ns,
            expiry_ns: self.expiry_ns,
            message_hash: self.message_hash,
            is_commit_or_proposal: self.is_commit_or_proposal,
        };
        Ok(api::ServerEnvelope {
            meta: Some(meta.wire()),
            envelope: Some(envelope),
        })
    }
}
