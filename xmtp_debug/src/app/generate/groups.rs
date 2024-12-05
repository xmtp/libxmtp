//! Group Generation
use crate::app::{
    store::{Database, GroupStore, IdentityStore, RandomDatabase},
    types::*,
};
use crate::{app, args};
use color_eyre::eyre::{self, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;

pub struct GenerateGroups {
    group_store: GroupStore<'static>,
    identity_store: IdentityStore<'static>,
    // metadata_store: MetadataStore<'static>,
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

    pub async fn create_groups(&self, n: usize, invitees: usize) -> Result<Vec<Group>> {
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
        for _ in 0..n {
            let identity = self.identity_store.random(network, &mut rng)?.unwrap();
            let invitees = self.identity_store.random_n(network, &mut rng, invitees)?;
            let bar_pointer = bar.clone();
            let network = network.clone();
            handles.push(set.spawn(async move {
                debug!(address = identity.address(), "group owner");
                let client = app::client_from_identity(&identity, &network).await?;
                let ids = invitees
                    .iter()
                    .map(|i| hex::encode(i.inbox_id))
                    .collect::<Vec<_>>();
                let group = client.create_group(Default::default(), Default::default())?;
                group.add_members_by_inbox_id(ids.as_slice()).await?;
                bar_pointer.inc(1);
                let mut members = invitees
                    .into_iter()
                    .map(|i| i.inbox_id)
                    .collect::<Vec<InboxId>>();
                members.push(identity.inbox_id);
                Ok(Group {
                    id: group
                        .group_id
                        .try_into()
                        .expect("Group id expected to be 32 bytes"),
                    member_size: members.len() as u32,
                    members,
                    created_by: identity.inbox_id,
                })
            }));

            // going above 128 we hit "unable to open database errors"
            // This may be related to open file limits
            if set.len() >= 64 {
                if let Some(Ok(group)) = set.join_next().await {
                    groups.push(group?);
                }
            }
        }

        while let Some(Ok(group)) = set.join_next().await {
            groups.push(group?);
        }
        self.group_store.set_all(groups.as_slice(), &self.network)?;
        Ok(groups)
    }
}
