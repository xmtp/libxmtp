//! Shared state for the health-check run.

use crate::DbgClient;
use crate::app::health::conditions::Conditions;
use crate::app::store::{
    Database, DeriveKey, GroupStore, IdentityKey, IdentityStore, MessageStore,
};
use crate::app::types::{Group, Identity, InboxId, Message};
use crate::app::{self, App, client_from_identity};
use color_eyre::eyre::{Result, ensure, eyre};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use xmtp_proto::types::GroupId;

/// Number of identities to create when the redb identity store is empty.
/// 3 is the minimum that lets us exercise group ops (one creator + two
/// others) and DM ops (primary ↔ peer with at least one extra identity).
const BOOTSTRAP_IDENTITY_COUNT: usize = 3;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct HealthClient {
    identity: Identity,
}

impl HealthClient {
    pub fn new(identity: Identity) -> Self {
        Self { identity }
    }

    fn realize(&self, store: &IdentityStore<'static>) -> Result<DbgClient> {
        let key = self.identity.key();
        if App::strict_versioning() {
            let current_version = App::current_version_hash();
            ensure!(
                current_version == key.version_hash,
                "cannot load identities from other versions with `--strict-versioning`."
            );
        }
        let identity = store
            .get(key)?
            .ok_or_else(|| eyre!("no identity found for {}", self.identity))?;
        client_from_identity(&identity)
    }
}

pub struct HealthContext {
    /// The single new identity created for this run. Persisted to the
    /// redb identity store so future runs see it as an existing identity.
    pub primary: HealthClient,

    id_store: IdentityStore<'static>,

    group_store: GroupStore<'static>,

    /// Identities loaded from the redb `IdentityStore` for this network.
    /// On a fresh run, contains the freshly-bootstrapped identities.
    pub existing_clients: HashSet<HealthClient>,

    /// Identities we do not have a client for (different
    /// `version_hash` partition), but which member-add ops should
    /// still include so cross-version groups stay in sync.
    pub other_identities: HashSet<HealthClient>,

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
    /// With `read_only`, primary is reused from `existing_clients`
    /// instead of registering a new on-network identity.
    pub async fn bootstrap(read_only: bool, conditions: &mut Conditions) -> Result<Self> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.into();

        // 1. Existing identities. Walk the full id_store once to count
        //    and to collect non-current-version inboxes (those we won't
        //    build clients for, but which member-add ops still include).
        let current_vh = App::current_version_hash();
        let other = if let Some(ids) = id_store.load_for_other_versions(current_vh)? {
            ids.map(|v| HealthClient::new(v.value()))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };

        let existing = if let Some(ids) = id_store.load_for_version(current_vh)? {
            ids.map(|v| HealthClient::new(v.value()))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };

        let empty = other.is_empty() && existing.is_empty();
        if read_only && empty {
            tracing::info!("no identities to check.");
            std::process::exit(0);
        }

        if empty {
            conditions.insert(Conditions::BOOTSTRAP);
        }
        // if empty {
        //     tracing::info!(
        //         target: "healthcheck",
        //         count = BOOTSTRAP_IDENTITY_COUNT,
        //         "redb identity store empty; creating bootstrap identities",
        //     );
        //     let mut fresh_identities: Vec<Identity> = Vec::new();
        //     for _ in 0..BOOTSTRAP_IDENTITY_COUNT {
        //         let wallet = app::generate_wallet();
        //         let client = app::new_unregistered_client(Some(&wallet)).await?;
        //         let identity = Identity::from_libxmtp(client.identity(), wallet.clone())?;
        //         app::register_client(&client, wallet.into_alloy()).await?;
        //         let inbox_id = identity.inbox_id;
        //         fresh_identities.push(identity);
        //         existing_clients.insert(inbox_id, Arc::new(client));
        //     }
        //     id_store.set_all(fresh_identities.as_slice())?;
        // } else {
        //     let loaded = app::load_all_identities(&id_store)?;
        //     let map = Arc::try_unwrap(loaded).map_err(|arc| {
        //         color_eyre::eyre::eyre!(
        //             "load_all_identities returned multiply-owned Arc (strong={}, weak={}). \
        //              HealthContext::bootstrap must be its sole caller — something is holding a clone.",
        //             Arc::strong_count(&arc),
        //             Arc::weak_count(&arc),
        //         )
        //     })?;
        //     for (inbox_id, mutex) in map.into_iter() {
        //         existing_clients.insert(inbox_id, Arc::new(mutex.into_inner()));
        //     }
        // }

        // // 2. Primary identity. Register fresh + persist, or reuse
        // //    existing under read_only.
        // let primary = if read_only {
        //     existing_clients.values().next().cloned().ok_or_else(|| {
        //         eyre!("read_only healthcheck requires at least one existing client")
        //     })?
        // } else {
        //     let primary_wallet = app::generate_wallet();
        //     let primary_client = app::new_unregistered_client(Some(&primary_wallet)).await?;
        //     let primary_identity =
        //         Identity::from_libxmtp(primary_client.identity(), primary_wallet.clone())?;
        //     app::register_client(&primary_client, primary_wallet.into_alloy()).await?;
        //     id_store.set(primary_identity)?;
        //     Arc::new(primary_client)
        // };

        // let existing_groups: Vec<GroupId> = if read_only {
        //     primary
        //         .find_groups(xmtp_db::group::GroupQueryArgs {
        //             conversation_type: Some(xmtp_db::group::ConversationType::Group),
        //             ..Default::default()
        //         })
        //         .unwrap_or_default()
        //         .into_iter()
        //         .map(|g| GroupId::from(g.group_id))
        //         .collect()
        // } else {
        //     let group_only_ids: std::collections::HashSet<GroupId> = existing_clients
        //         .values()
        //         .flat_map(|c| {
        //             c.find_groups(xmtp_db::group::GroupQueryArgs {
        //                 conversation_type: Some(xmtp_db::group::ConversationType::Group),
        //                 ..Default::default()
        //             })
        //             .unwrap_or_default()
        //             .into_iter()
        //             .map(|g| GroupId::from(g.group_id))
        //         })
        //         .collect();
        //     group_store
        //         .load()?
        //         .map(|iter| {
        //             iter.filter_map(|g| {
        //                 let id = g.value().id();
        //                 group_only_ids.contains(&id).then_some(id)
        //             })
        //             .collect::<Vec<_>>()
        //         })
        //         .unwrap_or_default()
        // };

        // tracing::info!(
        //     target: "healthcheck",
        //     existing_identities = existing_clients.len(),
        //     other_identities = other_identities.len(),
        //     existing_groups = existing_groups.len(),
        //     "health context bootstrapped"
        // );

        let ctx = Self {
            primary,
            existing_clients: existing,
            other_identities: other,
            existing_groups,
            new_groups: Vec::new(),
            group_store,
            id_store,
        };
        // Drain welcomes left server-side by prior cross-version runs;
        // without this the first `client.group(<gid>)` hits "not found".
        // ctx.sync_welcomes_fanout("bootstrap").await;
        Ok(ctx)
    }

    pub fn primary(&self) -> Result<DbgClient> {
        self.primary.realize(&self.id_store)
    }

    /// Register a fresh single-use identity. Not persisted to redb. Used
    /// by ops that need a victim/leaver they own end-to-end — e.g.
    /// `LeaveGroup` adds + leaves; `RemoveMember` adds + admin-removes.
    /// The returned `Arc<DbgClient>` is owned by the caller; once dropped,
    /// nothing in the run references it.
    pub async fn create_transient(&self) -> Result<Arc<DbgClient>> {
        let wallet = app::generate_wallet();
        let client = app::new_unregistered_client(Some(&wallet)).await?;
        app::register_client(&client, wallet.into_alloy()).await?;
        Ok(Arc::new(client))
    }

    /// Persist a newly-created group to redb's `GroupStore` so subsequent
    /// healthcheck runs (potentially on a different libxmtp version) see
    /// it as an existing group. Called by `CreateGroup` after the MLS
    /// group is created on the network.
    ///
    /// Panics if redb can't be opened or if `group_id` isn't 16 bytes —
    /// both indicate state-dir / invariant problems, not op-level
    /// failures, and the run can't continue meaningfully.
    pub fn persist_new_group(
        &self,
        group_id: &GroupId,
        created_by: InboxId,
        members: Vec<InboxId>,
    ) {
        let group_store: GroupStore<'static> = redb_or_panic("persist_new_group").into();
        group_store
            .set(Group::new(*group_id, created_by, members, false))
            .expect("redb GroupStore::set failed");
    }

    pub fn persist_new_dm(&self, group_id: &GroupId, created_by: InboxId, members: Vec<InboxId>) {
        let group_store: GroupStore<'static> = redb_or_panic("persist_new_group").into();
        group_store
            .set(Group::new(*group_id, created_by, members, true))
            .expect("redb GroupStore::set failed");
    }

    /// Replace a persisted group's member list. Used by membership ops
    /// (`AddMembersToNewGroup`, `AddPrimaryToExistingGroups`,
    /// `RemoveMember`, `LeaveGroup`) so redb's view stays consistent with
    /// the MLS-level state across runs.
    ///
    /// Panics on redb failure or non-16-byte `group_id`.
    pub fn update_group_members(&self, group_id: &GroupId, members: Vec<InboxId>) {
        let group_store: GroupStore<'static> = redb_or_panic("update_group_members").into();
        // Preserve `created_by` if a row already exists; otherwise default
        // to zero — the field is informational, not load-bearing.
        let created_by = group_store
            .get(group_id.into())
            .expect("redb GroupStore::get failed")
            .map(|g| g.created_by)
            .unwrap_or([0u8; 32]);
        group_store
            .set(Group::new(*group_id, created_by, members))
            .expect("redb GroupStore::set failed");
    }

    /// Look up a persisted group's current members, if it's recorded.
    /// Returns an empty vec if the group is not in redb.
    pub fn persisted_members(&self, group_id: &GroupId) -> Vec<InboxId> {
        let group_store: GroupStore<'static> = redb_or_panic("persisted_members").into();
        group_store
            .get(group_id.into())
            .expect("redb GroupStore::get failed")
            .map(|g| g.members)
            .unwrap_or_default()
    }

    /// Every persisted client involved in this run: primary + every
    /// entry in `existing_clients`. Run-scoped throwaway transients
    /// created by ops are NOT included — they're op-local. Under
    /// `read_only`, primary is one of `existing_clients` — deduped
    /// here so it isn't processed twice.
    pub fn all_clients(&self) -> Vec<Arc<DbgClient>> {
        let primary_inbox = self.primary.inbox_id().to_string();
        let mut out = Vec::with_capacity(self.existing_clients.len() + 1);
        out.push(self.primary.clone());
        out.extend(
            self.existing_clients
                .iter()
                .filter(|(_, c)| c.inbox_id() != primary_inbox)
                .map(|(_, c)| c.clone()),
        );
        out
    }

    /// Every group this run cares about: existing (loaded from redb)
    /// followed by new groups created by ops in this run.
    pub fn all_groups(&self) -> impl Iterator<Item = &GroupId> {
        self.existing_groups.iter().chain(self.new_groups.iter())
    }

    /// Pick an active adder for a membership change; prefer super-admin so
    /// `AddSuper` is also issuable, else any active member (under
    /// `--strict-versioning` the creator may live in another partition).
    pub fn pick_super_admin(
        &self,
        group_id: &GroupId,
    ) -> Option<xmtp_mls::groups::MlsGroup<crate::MlsContext>> {
        let candidates: Vec<_> = self
            .existing_clients
            .values()
            .filter_map(|c| c.group(group_id).ok().map(|g| (c, g)))
            .filter(|(_, g)| g.is_active().unwrap_or(false))
            .collect();
        candidates
            .iter()
            .find(|(c, g)| g.is_super_admin(c.inbox_id().to_string()).unwrap_or(false))
            .or_else(|| candidates.first())
            .map(|(_, g)| g.clone())
    }

    /// Best-effort concurrent welcome sync across all run clients;
    /// failures are logged only (plumbing, not a validation step).
    pub async fn sync_welcomes_fanout(&self, label: &'static str) {
        let clients = self.all_clients();
        let syncs = clients.iter().map(|c| async move {
            if let Err(e) = c.sync_welcomes().await {
                tracing::warn!(
                    target: "healthcheck",
                    inbox = c.inbox_id(),
                    error = %e,
                    label,
                    "sync_welcomes fanout failed",
                );
            }
        });
        futures::future::join_all(syncs).await;
    }

    /// Record a successfully-sent message to redb's `MessageStore`. The
    /// xdbg_version` fields is sourced from `self` so
    /// every row carries this run's identity. Panics on redb failure or
    /// on a non-16-byte `group_id` (libxmtp invariant).
    pub fn record_message(&self, group_id: &GroupId, message_id: [u8; 32], sender: &DbgClient) {
        let sent_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let store: MessageStore<'static> = redb_or_panic("record_message").into();
        let msg = Message::new(
            message_id,
            *group_id,
            inbox_id_to_bytes(sender.inbox_id()),
            sent_at_ns,
        );
        store.set(msg).expect("redb MessageStore::set failed");
    }

    /// Load every recorded message for this network and bucket by
    /// `group_id`. Single scan; the validator calls this once and reads
    /// per-group sub-vecs out of the map.
    pub fn recorded_messages_by_group(&self) -> HashMap<GroupId, Vec<Message>> {
        let store: MessageStore<'static> = redb_or_panic("recorded_messages_by_group").into();
        let Some(iter) = store.load().expect("redb MessageStore::load failed") else {
            return HashMap::new();
        };
        let mut out: HashMap<GroupId, Vec<Message>> = HashMap::new();
        for guard in iter {
            let msg = guard.value();
            out.entry(msg.group_id()).or_default().push(msg);
        }
        out
    }
}

/// Decode the libxmtp hex inbox_id into the 32-byte form xdbg's redb uses
/// as a `HashMap` / `IdentityStore` key. The hex form is guaranteed valid
/// by libxmtp, so we panic on malformed input.
pub fn inbox_id_to_bytes(hex_inbox: &str) -> InboxId {
    let mut out = [0u8; 32];
    hex::decode_to_slice(hex_inbox, &mut out).expect("inbox_id is 32-byte hex");
    out
}

/// Open the xdbg redb database or abort the process. Failure here means
/// xdbg's state directory is broken — not an op-level failure — so
/// trying to keep going would produce misleading healthcheck results.
fn redb_or_panic(caller: &str) -> Arc<redb::Database> {
    App::db()
        .unwrap_or_else(|e| panic!("healthcheck::{caller}: failed to open redb database: {e:#}"))
}
