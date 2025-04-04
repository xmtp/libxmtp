use std::sync::Arc;

use crate::args;
use color_eyre::eyre::{Result, eyre};
use rand::prelude::*;

use super::store::{Database, GroupStore, IdentityStore};

#[derive(Debug)]
pub struct Send {
    db: Arc<redb::Database>,
    opts: args::Send,
    network: args::BackendOpts,
}

impl Send {
    pub fn new(opts: args::Send, network: args::BackendOpts, db: Arc<redb::Database>) -> Self {
        Self { opts, network, db }
    }

    pub async fn run(self) -> Result<()> {
        use args::ActionKind::*;
        let args::Send { ref action, .. } = self.opts;

        match action {
            Message => self.message().await,
        }
    }

    async fn message(&self) -> Result<()> {
        let Self { network, .. } = self;
        let args::Send { data, group_id, .. } = &self.opts;

        let group_store: GroupStore = self.db.clone().into();
        let identity_store: IdentityStore = self.db.clone().into();
        let key = (u64::from(network), **group_id);
        let group = group_store
            .get(key.into())?
            .ok_or(eyre!("No group with id {}", group_id))?;
        let member = group
            .members
            .choose(&mut rand::thread_rng())
            .ok_or(eyre!("Empty group, no members to send message!"))?;
        let key = (u64::from(network), *member);
        let identity = identity_store
            .get(key.into())?
            .ok_or(eyre!("No Identity with inbox_id [{}]", hex::encode(member)))?;

        let client = crate::app::client_from_identity(&identity, network).await?;
        let provider = client.mls_provider()?;
        client.sync_welcomes(&provider).await?;
        let xmtp_group = client.group(group.id.to_vec())?;
        xmtp_group.send_message(data.as_bytes()).await?;
        Ok(())
    }
}
