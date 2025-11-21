//! Group Generation
use crate::app::load_n_identities;
use crate::app::{
    store::{Database, GroupStore, IdentityStore, RandomDatabase},
    types::*,
};
use crate::args;
use color_eyre::eyre::{self, Result, ensure, eyre};
use futures::{StreamExt, TryFutureExt, TryStreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::types::InstallationId;

pub struct GenerateGroups {
    group_store: GroupStore<'static>,
    identity_store: IdentityStore<'static>,
    network: args::BackendOpts,
}

impl GenerateGroups {
    pub fn new(db: Arc<redb::Database>, network: args::BackendOpts) -> Self {
        Self {
            group_store: db.clone().into(),
            identity_store: db.clone().into(),
            // metadata_store: db.clone().into(),
            network,
        }
    }

    pub async fn create_groups(
        &self,
        n: usize,
        invitees: usize,
        concurrency: usize,
    ) -> Result<Vec<Group>> {
        tracing::info!("creating groups");

        let network = &self.network;
        let identities = self.identity_store.len(network)?;
        ensure!(
            identities >= invitees,
            "not enough identities generated. have {}, but need to invite {}. groups cannot hold duplicate identities",
            identities,
            invitees
        );
        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());

        let clients = load_n_identities(&self.identity_store, network, n)?;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let groups = stream::iter(clients.iter())
            .map(|(owner, client)| {
                let bar_pointer = bar.clone();
                let client = client.clone();
                let owner = *owner;
                let store = self.identity_store.clone();
                let network = u64::from(network);
                let semaphore = semaphore.clone();
                Ok(tokio::spawn({
                    async move {
                        let _permit = semaphore.acquire().await?;
                        debug!(owner = hex::encode(owner), "group owner");
                        let invitees = {
                            let mut rng = rand::thread_rng();
                            // todo: maybe generate more identities at this point?
                            // or earlier, check if we have sufficient identities for this
                            // command
                            store.random_n_capped(network, &mut rng, invitees)
                        }?;
                        let mut ids = Vec::with_capacity(invitees.len());
                        for member in &invitees {
                            let member = member.value();
                            let cred =
                                XmtpInstallationCredential::from_bytes(&member.installation_key)?;
                            let inbox_id = hex::encode(member.inbox_id);
                            tracing::debug!(
                                inbox_ids = hex::encode(member.inbox_id),
                                installation_key = %InstallationId::from(*cred.public_bytes()),
                                "Adding Members"
                            );
                            ids.push(inbox_id);
                        }
                        let client = client.lock().await;
                        let group = client.create_group(Default::default(), Default::default())?;
                        group.add_members_by_inbox_id(ids.as_slice()).await?;
                        bar_pointer.inc(1);
                        let mut members = invitees
                            .into_iter()
                            .map(|i| i.value().inbox_id)
                            .collect::<Vec<InboxId>>();
                        members.push(owner);
                        Ok(Group {
                            id: group
                                .group_id
                                .try_into()
                                .expect("Group id expected to be 32 bytes"),
                            member_size: members.len() as u32,
                            members,
                            created_by: owner,
                        })
                    }
                })
                .map_err(|_| eyre!("failed to spawn tasks for group creation")))
            })
            .try_buffer_unordered(concurrency)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, eyre::Report>>()?;
        self.group_store.set_all(groups.as_slice(), &self.network)?;
        // ensure cleanup for each client
        for client in clients.values() {
            let client = client.lock().await;
            client.release_db_connection()?;
        }
        Ok(groups)
    }
}
