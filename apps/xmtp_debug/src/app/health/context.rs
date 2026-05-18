//! Shared state for the health-check run.

use crate::DbgClient;
use crate::app::store::{Database, GroupStore, IdentityStore, MessageStore};
use crate::app::types::{Group, Identity, InboxId, Message};
use crate::app::{self, App};
use crate::args;
use color_eyre::eyre::{Result, eyre};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use xmtp_proto::types::GroupId;

/// Number of identities to create when the redb identity store is empty.
/// 3 is the minimum that lets us exercise group ops (one creator + two
/// others) and DM ops (primary ↔ peer with at least one extra identity).
const BOOTSTRAP_IDENTITY_COUNT: usize = 3;

pub struct HealthContext {
    /// Network selection. Held so ops can re-open `GroupStore` to persist
    /// newly-created groups for future runs.
    pub network: args::BackendOpts,

    /// The single new identity created for this run. Persisted to the
    /// redb identity store so future runs see it as an existing identity.
    pub primary: Arc<DbgClient>,

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

    /// Random 16-byte id generated at bootstrap. Stamped on every
    /// recorded `Message` so cross-version tooling can attribute messages
    /// to specific runs without scanning logs.
    pub op_run_id: [u8; 16],

    /// `crate::get_version()` output of the running xdbg binary. Stamped
    /// alongside `op_run_id` so a recovered redb row carries enough
    /// origin info for debugging.
    pub xdbg_version: String,

    /// The active network. Held so helpers (`record_message`,
    /// `recorded_messages`) can re-open `MessageStore` without threading
    /// network through every call site.
    pub network_key: u64,
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

        // 3. Existing groups from redb.
        let existing_groups = group_store
            .load(net_key)?
            .map(|iter| {
                iter.map(|g| GroupId::from(g.value().id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        tracing::info!(
            target: "healthcheck",
            existing_identities = existing_clients.len(),
            existing_groups = existing_groups.len(),
            "health context bootstrapped"
        );

        let op_run_id: [u8; 16] = rand::random();
        let xdbg_version = crate::get_version();

        Ok(Self {
            network,
            primary,
            existing_clients,
            existing_groups,
            new_groups: Vec::new(),
            op_run_id,
            xdbg_version,
            network_key: net_key,
        })
    }

    /// Register a fresh single-use identity. Not persisted to redb. Used
    /// by ops that need a victim/leaver they own end-to-end — e.g.
    /// `LeaveGroup` adds + leaves; `RemoveMember` adds + admin-removes.
    /// The returned `Arc<DbgClient>` is owned by the caller; once dropped,
    /// nothing in the run references it.
    pub async fn create_transient(&self) -> Result<Arc<DbgClient>> {
        let wallet = app::generate_wallet();
        let client = app::new_unregistered_client(&self.network, Some(&wallet)).await?;
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
        let id = group_id_bytes(group_id).expect("libxmtp group_id is 16 bytes");
        let group_store: GroupStore<'static> = redb_or_panic("persist_new_group").into();
        let group = Group {
            id,
            created_by,
            member_size: members.len() as u32,
            members,
        };
        group_store
            .set(group, u64::from(&self.network))
            .expect("redb GroupStore::set failed");
    }

    /// Replace a persisted group's member list. Used by membership ops
    /// (`AddMembersToNewGroup`, `AddPrimaryToExistingGroups`,
    /// `RemoveMember`, `LeaveGroup`) so redb's view stays consistent with
    /// the MLS-level state across runs.
    ///
    /// Panics on redb failure or non-16-byte `group_id`.
    pub fn update_group_members(&self, group_id: &GroupId, members: Vec<InboxId>) {
        let id = group_id_bytes(group_id).expect("libxmtp group_id is 16 bytes");
        let group_store: GroupStore<'static> = redb_or_panic("update_group_members").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
        // Preserve `created_by` if a row already exists; otherwise default
        // to zero — the field is informational, not load-bearing.
        let created_by = group_store
            .get(key)
            .expect("redb GroupStore::get failed")
            .map(|g| g.created_by)
            .unwrap_or([0u8; 32]);
        let group = Group {
            id,
            created_by,
            member_size: members.len() as u32,
            members,
        };
        group_store
            .set(group, net_key)
            .expect("redb GroupStore::set failed");
    }

    /// Look up a persisted group's current members, if it's recorded.
    /// Returns an empty vec if the group is not in redb.
    pub fn persisted_members(&self, group_id: &GroupId) -> Vec<InboxId> {
        let Ok(id) = group_id_bytes(group_id) else {
            return Vec::new();
        };
        let group_store: GroupStore<'static> = redb_or_panic("persisted_members").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
        group_store
            .get(key)
            .expect("redb GroupStore::get failed")
            .map(|g| g.members)
            .unwrap_or_default()
    }

    /// Read the persisted `created_by` for a group. Returns `None` when
    /// no row exists, `group_id` isn't 16 bytes, or `created_by` is the
    /// all-zero placeholder written by `update_group_members` for groups
    /// created before persistence was added.
    pub fn persisted_creator(&self, group_id: &GroupId) -> Option<InboxId> {
        let id = group_id_bytes(group_id).ok()?;
        let group_store: GroupStore<'static> = redb_or_panic("persisted_creator").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
        group_store
            .get(key)
            .expect("redb GroupStore::get failed")
            .map(|g| g.created_by)
            .filter(|c| c != &[0u8; 32])
    }

    /// Every persisted client involved in this run: primary + every
    /// entry in `existing_clients`. Run-scoped throwaway transients
    /// created by ops are NOT included — they're op-local.
    pub fn all_clients(&self) -> Vec<Arc<DbgClient>> {
        let mut out = Vec::with_capacity(self.existing_clients.len() + 1);
        out.push(self.primary.clone());
        out.extend(self.existing_clients.values().cloned());
        out
    }

    /// Every group this run cares about: existing (loaded from redb)
    /// followed by new groups created by ops in this run.
    pub fn all_groups(&self) -> impl Iterator<Item = &GroupId> {
        self.existing_groups.iter().chain(self.new_groups.iter())
    }

    /// Concurrently sync welcomes on every client involved in the run.
    /// Failures are logged but never propagated — `sync_welcomes` is
    /// best-effort plumbing, not a validation step.
    pub async fn sync_welcomes_fanout(&self, label: &'static str) {
        let syncs = std::iter::once(self.primary.as_ref())
            .chain(self.existing_clients.values().map(|c| c.as_ref()))
            .map(|c| async move {
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
    /// `op_run_id` and `xdbg_version` fields are sourced from `self` so
    /// every row carries this run's identity. Panics on redb failure or
    /// on a non-16-byte `group_id` (libxmtp invariant).
    pub fn record_message(&self, group_id: &GroupId, message_id: [u8; 32], sender: &DbgClient) {
        let id_bytes = group_id_bytes(group_id).expect("libxmtp group_id is 16 bytes");
        let sent_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let store: MessageStore<'static> = redb_or_panic("record_message").into();
        let msg = Message {
            id: message_id,
            group_id: id_bytes,
            sender_inbox_id: inbox_id_to_bytes(sender.inbox_id()),
            sent_at_ns,
            op_run_id: self.op_run_id,
            xdbg_version: self.xdbg_version.clone(),
        };
        store
            .set(msg, self.network_key)
            .expect("redb MessageStore::set failed");
    }

    /// Load every recorded message for this network and bucket by
    /// `group_id`. Single scan; the validator calls this once and reads
    /// per-group sub-vecs out of the map.
    pub fn recorded_messages_by_group(&self) -> HashMap<GroupId, Vec<Message>> {
        let store: MessageStore<'static> = redb_or_panic("recorded_messages_by_group").into();
        let Some(iter) = store
            .load(self.network_key)
            .expect("redb MessageStore::load failed")
        else {
            return HashMap::new();
        };
        let mut out: HashMap<GroupId, Vec<Message>> = HashMap::new();
        for guard in iter {
            let msg = guard.value();
            out.entry(GroupId::from(msg.group_id))
                .or_default()
                .push(msg);
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

/// Convert a libxmtp `GroupId` to the 16-byte form xdbg's redb uses as a
/// key. libxmtp's MLS group ids are always 16 bytes; an unexpected length
/// is an invariant violation, surfaced as an op-level error so the run
/// reports the divergence instead of silently skipping the redb write.
pub fn group_id_bytes(gid: &GroupId) -> color_eyre::eyre::Result<[u8; 16]> {
    <[u8; 16]>::try_from(gid.as_slice()).map_err(|_| {
        eyre!(
            "expected 16-byte group_id, got {} bytes",
            gid.as_slice().len()
        )
    })
}

/// Open the xdbg redb database or abort the process. Failure here means
/// xdbg's state directory is broken — not an op-level failure — so
/// trying to keep going would produce misleading healthcheck results.
fn redb_or_panic(caller: &str) -> Arc<redb::Database> {
    App::db()
        .unwrap_or_else(|e| panic!("healthcheck::{caller}: failed to open redb database: {e:#}"))
}
