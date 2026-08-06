use std::sync::Arc;

use crate::{app::App, args};
use color_eyre::eyre::{Result, eyre};
use rand::prelude::*;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;

use super::store::{Database, GroupStore, IdentityStore};

pub struct Send {
    db: Arc<redb::ReadOnlyDatabase>,
    opts: &'static args::Send,
}

impl Send {
    pub fn new(opts: &'static args::Send) -> Result<Self> {
        let db = App::readonly_db()?;
        Ok(Self { opts, db })
    }

    pub async fn run(self) -> Result<()> {
        use args::ActionKind::*;
        let args::Send { action, .. } = self.opts;

        match action {
            Message => self.message().await,
        }
    }

    async fn message(&self) -> Result<()> {
        let args::Send { data, group_id, .. } = &self.opts;

        let group_store: GroupStore = self.db.clone().into();
        let identity_store: IdentityStore = self.db.clone().into();
        let group = group_store
            .get(group_id.into())?
            .ok_or(eyre!("No group with id {}", group_id))?;
        let member = group
            .members
            .choose(&mut rand::rng())
            .ok_or(eyre!("Empty group, no members to send message!"))?;
        let identity = identity_store
            .find_by_inbox(*member)?
            .ok_or(eyre!("No Identity with inbox_id [{}]", hex::encode(member)))?;

        let client = crate::app::client_from_identity(&identity)?;
        client.sync_welcomes().await?;
        let xmtp_group = client.group(&group.id())?;
        xmtp_group
            .send_message(
                data.as_bytes(),
                SendMessageOptsBuilder::default()
                    .should_push(true)
                    .build()
                    .unwrap(),
            )
            .await?;
        Ok(())
    }
}
