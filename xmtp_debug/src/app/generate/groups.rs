//! Group Generation
use crate::app::load_all_identities;
use crate::app::{
    store::{Database, GroupStore, IdentityStore, RandomDatabase},
    types::*,
};
use crate::args;
use color_eyre::eyre::{self, Result, eyre};
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::IteratorRandom;
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

    #[allow(unused)]
    pub fn load_groups(&self) -> Result<Option<impl Iterator<Item = Result<Group>> + use<'_>>> {
        Ok(self
            .group_store
            .load(&self.network)?
            .map(|i| i.map(|i| Ok(i.value()))))
    }

    pub async fn create_groups(
        &self,
        n: usize,
        invitees: usize,
        concurrency: usize,
    ) -> Result<Vec<Group>> {
        // TODO: Check if identities still exist
        let mut groups: Vec<Group> = Vec::with_capacity(n);
        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());
        let mut set: tokio::task::JoinSet<Result<_, eyre::Error>> = tokio::task::JoinSet::new();
        let mut handles = vec![];

        let network = &self.network;
        let mut rng = rand::thread_rng();

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        let clients = load_all_identities(&self.identity_store, network)?;
        for _ in 0..n {
            let invitees = self.identity_store.random_n(network, &mut rng, invitees)?;
            let bar_pointer = bar.clone();
            let semaphore = semaphore.clone();
            let cs = clients.clone();
            handles.push(set.spawn(async move {
                let _permit = semaphore.acquire().await?;
                let owner = cs.keys().choose(&mut rand::thread_rng()).ok_or(eyre!(
                    "no local identities found in database, have identities been generated?"
                ))?;
                debug!(owner = hex::encode(owner), "group owner");
                let mut ids = Vec::with_capacity(invitees.len());
                for member in &invitees {
                    let cred = XmtpInstallationCredential::from_bytes(&member.installation_key)?;
                    let inbox_id = hex::encode(member.inbox_id);
                    tracing::debug!(
                        inbox_ids = hex::encode(member.inbox_id),
                        installation_key = %InstallationId::from(*cred.public_bytes()),
                        "Adding Members"
                    );
                    ids.push(inbox_id);
                }
                let client = cs.get(owner).unwrap().lock().await;
                let group = client.create_group(Default::default(), Default::default())?;
                group.add_members_by_inbox_id(ids.as_slice()).await?;
                bar_pointer.inc(1);
                let mut members = invitees
                    .into_iter()
                    .map(|i| i.inbox_id)
                    .collect::<Vec<InboxId>>();
                members.push(*owner);
                Ok(Group {
                    id: group
                        .group_id
                        .try_into()
                        .expect("Group id expected to be 32 bytes"),
                    member_size: members.len() as u32,
                    members,
                    created_by: *owner,
                })
            }));

            // going above 128 we hit "unable to open database errors"
            // This may be related to open file limits
            if set.len() >= 64
                && let Some(group) = set.join_next().await
            {
                match group {
                    Ok(group) => {
                        groups.push(group?);
                    }
                    Err(e) => {
                        error!("{}", e.to_string());
                    }
                }
            }
        }

        while let Some(group) = set.join_next().await {
            match group {
                Ok(group) => {
                    groups.push(group?);
                }
                Err(e) => {
                    error!("{}", e.to_string());
                }
            }
        }
        self.group_store.set_all(groups.as_slice(), &self.network)?;
        Ok(groups)
    }
}
