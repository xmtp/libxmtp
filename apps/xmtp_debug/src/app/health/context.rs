//! Shared state for the health-check run.

use crate::DbgClient;
use crate::app::store::{Database, GroupStore, IdentityStore};
use crate::app::types::{Identity, InboxId};
use crate::app::{self, App};
use crate::args;
use color_eyre::eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;
use xmtp_proto::types::GroupId;

/// Number of identities to create when the redb identity store is empty.
/// 3 is the minimum that lets us exercise group ops (one creator + two
/// others) and DM ops (primary ↔ peer with at least one extra identity).
const BOOTSTRAP_IDENTITY_COUNT: usize = 3;

pub struct HealthContext {
    /// The single new identity created for this run. Persisted to the
    /// redb identity store so future runs see it as an existing identity.
    pub primary: Arc<DbgClient>,

    /// Single-run identity used by destructive ops (e.g. `LeaveGroup`).
    /// Not persisted to redb — exists only to be removed from groups
    /// without losing the run-stable `primary`.
    pub transient_identity: Arc<DbgClient>,

    /// Identities loaded from the redb `IdentityStore` for this network.
    /// On a fresh run, contains the freshly-bootstrapped identities.
    pub existing_clients: HashMap<InboxId, Arc<DbgClient>>,

    /// Group IDs loaded from the redb `GroupStore` for this network.
    /// Stored as the raw 16-byte ids as produced by libxmtp.
    pub existing_groups: Vec<GroupId>,

    /// Groups created by ops during this run. Ops receive `&mut
    /// HealthContext` and mutate this directly — execution is sequential
    /// so no synchronization is needed.
    pub new_groups: Vec<GroupId>,
}

impl HealthContext {
    /// Build the full context: load existing state, bootstrap on fresh DB,
    /// create the primary identity. Hard-fails on infrastructure errors.
    pub async fn bootstrap(network: args::BackendOpts) -> Result<Self> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.into();
        let net_key = u64::from(&network);

        // 1. Existing identities.
        let identity_count = id_store
            .load(net_key)?
            .map(|iter| iter.count())
            .unwrap_or(0);

        let mut existing_clients: HashMap<InboxId, Arc<DbgClient>> = HashMap::new();

        if identity_count == 0 {
            tracing::info!(
                target: "healthcheck",
                count = BOOTSTRAP_IDENTITY_COUNT,
                "redb identity store empty; creating bootstrap identities",
            );
            let mut fresh_identities: Vec<Identity> = Vec::new();
            for _ in 0..BOOTSTRAP_IDENTITY_COUNT {
                let wallet = app::generate_wallet();
                let client = app::new_unregistered_client(&network, Some(&wallet)).await?;
                let identity = Identity::from_libxmtp(client.identity(), wallet.clone())?;
                app::register_client(&client, wallet.into_alloy()).await?;
                let inbox_id = identity.inbox_id;
                fresh_identities.push(identity);
                existing_clients.insert(inbox_id, Arc::new(client));
            }
            id_store.set_all(fresh_identities.as_slice(), net_key)?;
        } else {
            let loaded = app::load_all_identities(&id_store, &network)?;
            let map = Arc::try_unwrap(loaded).map_err(|arc| {
                color_eyre::eyre::eyre!(
                    "load_all_identities returned multiply-owned Arc (strong={}, weak={}). \
                     HealthContext::bootstrap must be its sole caller — something is holding a clone.",
                    Arc::strong_count(&arc),
                    Arc::weak_count(&arc),
                )
            })?;
            for (inbox_id, mutex) in map.into_iter() {
                existing_clients.insert(inbox_id, Arc::new(mutex.into_inner()));
            }
        }

        // 2. Primary new identity. Also persisted to redb so subsequent
        //    healthcheck runs see it as an existing identity.
        let primary_wallet = app::generate_wallet();
        let primary_client = app::new_unregistered_client(&network, Some(&primary_wallet)).await?;
        let primary_identity =
            Identity::from_libxmtp(primary_client.identity(), primary_wallet.clone())?;
        app::register_client(&primary_client, primary_wallet.into_alloy()).await?;
        id_store.set(primary_identity, net_key)?;
        let primary = Arc::new(primary_client);

        // 3. Transient identity for destructive ops. Not persisted.
        let transient_wallet = app::generate_wallet();
        let transient_client =
            app::new_unregistered_client(&network, Some(&transient_wallet)).await?;
        app::register_client(&transient_client, transient_wallet.into_alloy()).await?;
        let transient_identity = Arc::new(transient_client);

        // 4. Existing groups from redb.
        let existing_groups = group_store
            .load(net_key)?
            .map(|iter| {
                iter.map(|g| GroupId::from(g.value().id.as_slice()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        tracing::info!(
            target: "healthcheck",
            existing_identities = existing_clients.len(),
            existing_groups = existing_groups.len(),
            "health context bootstrapped"
        );

        Ok(Self {
            primary,
            transient_identity,
            existing_clients,
            existing_groups,
            new_groups: Vec::new(),
        })
    }

    /// Every client involved in this run: primary + transient + every
    /// entry in `existing_clients`. The two run-scoped clients are
    /// returned first so consumers can match on inbox_id if needed.
    pub fn all_clients(&self) -> Vec<Arc<DbgClient>> {
        let mut out = Vec::with_capacity(self.existing_clients.len() + 2);
        out.push(self.primary.clone());
        out.push(self.transient_identity.clone());
        out.extend(self.existing_clients.values().cloned());
        out
    }

    /// Every group this run cares about: existing (loaded from redb)
    /// followed by new groups created by ops in this run.
    pub fn all_groups(&self) -> impl Iterator<Item = &GroupId> {
        self.existing_groups.iter().chain(self.new_groups.iter())
    }
}
