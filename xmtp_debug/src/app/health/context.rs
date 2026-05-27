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

pub struct HealthContext {
    /// Network selection. Held so ops can re-open `GroupStore` to persist
    /// newly-created groups for future runs.
    pub network: args::BackendOpts,

    /// The single new identity created for this run. Persisted to the
    /// redb identity store so future runs see it as an existing identity.
    pub primary: Arc<DbgClient>,

    /// Extra new identities created only when the redb identity store
    /// was empty at startup. Empty on a non-fresh run.
    pub bootstrap_clients: Vec<Arc<DbgClient>>,

    /// Identities loaded from the redb `IdentityStore` for this network.
    /// On a fresh run, contains the registered `bootstrap_clients`.
    pub existing_clients: HashMap<InboxId, Arc<DbgClient>>,

    /// Identities we do not have a client for (different
    /// `version_hash` partition), but which member-add ops should
    /// still include so cross-version groups stay in sync.
    pub other_identities: Vec<InboxId>,

    /// Group IDs loaded from the redb `GroupStore` for this network.
    /// Stored as the raw 16-byte ids as produced by libxmtp.
    pub existing_groups: Vec<GroupId>,

    /// Groups created by ops during this run. Ops receive `&mut
    /// HealthContext` and mutate this directly — execution is sequential
    /// so no synchronization is needed.
    pub new_groups: Vec<GroupId>,

    /// `crate::get_version()` output of the running xdbg binary. Stamped
    /// on each recorded `Message` so a recovered redb row carries enough
    /// origin info for debugging.
    pub xdbg_version: String,

    /// The active network as `u64`. Held so helpers (`record_message`,
    /// `recorded_messages_by_group`) can re-open `MessageStore` without
    /// threading network through every call site.
    pub network_key: u64,
}

impl HealthContext {
    /// Build the full context: load existing state, bootstrap on fresh DB,
    /// create the primary identity. Hard-fails on infrastructure errors.
    ///
    /// `read_only`: skip primary-identity creation; pick an existing
    /// client as primary instead. Used by cross-talk-test rev-leg so
    /// no new on-network identity is registered when we're just
    /// verifying older clients can still read/send after newer writes.
    pub async fn bootstrap(
        network: args::BackendOpts,
        strict_versioning: bool,
        read_only: bool,
    ) -> Result<Self> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.into();
        let net_key = u64::from(&network);

        // 1. Existing identities. Walk the full id_store once to count
        //    and to collect non-current-version inboxes (those we won't
        //    build clients for, but which member-add ops still include).
        let current_vh = App::current_version_hash();
        let (identity_count, other_identities) = id_store
            .load(net_key)?
            .map(|iter| {
                iter.fold((0usize, Vec::<InboxId>::new()), |(n, mut others), guard| {
                    let id = guard.value();
                    if id.version_hash != current_vh {
                        others.push(id.inbox_id);
                    }
                    (n + 1, others)
                })
            })
            .unwrap_or((0, Vec::new()));

        let mut bootstrap_clients: Vec<Arc<DbgClient>> = Vec::new();
        let mut existing_clients: HashMap<InboxId, Arc<DbgClient>> = HashMap::new();

        if identity_count == 0 {
            tracing::info!(target: "healthcheck", "redb identity store empty; creating 3 bootstrap identities");
            let mut fresh_identities: Vec<Identity> = Vec::new();
            for _ in 0..3 {
                let wallet = app::generate_wallet();
                let client = app::new_unregistered_client(&network, Some(&wallet)).await?;
                let identity = Identity::from_libxmtp(client.identity(), wallet.clone())?;
                app::register_client(&client, wallet.into_alloy()).await?;
                let inbox_id = identity.inbox_id;
                let arc = Arc::new(client);
                fresh_identities.push(identity);
                bootstrap_clients.push(arc.clone());
                existing_clients.insert(inbox_id, arc);
            }
            id_store.set_all(fresh_identities.as_slice(), net_key)?;
        } else {
            let loaded = app::load_all_identities(&id_store, &network, strict_versioning)?;
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
        // Primary identity. Default: register a fresh wallet and
        // persist to redb. read_only: reuse an existing client so we
        // don't pollute the network with a throwaway identity that
        // won't be promoted/admin'd on any existing group.
        let primary = if read_only {
            existing_clients
                .values()
                .next()
                .cloned()
                .ok_or_else(|| eyre!("read_only healthcheck requires at least one existing client"))?
        } else {
            let primary_wallet = app::generate_wallet();
            let primary_client =
                app::new_unregistered_client(&network, Some(&primary_wallet)).await?;
            let primary_identity =
                Identity::from_libxmtp(primary_client.identity(), primary_wallet.clone())?;
            app::register_client(&primary_client, primary_wallet.into_alloy()).await?;
            id_store.set(primary_identity, net_key)?;
            Arc::new(primary_client)
        };

        // 3. Drain pending welcomes BEFORE filtering existing_groups.
        // A prior cross-version run (e.g. v1.10 healthcheck's
        // AddMembersToNewGroup) may have welcomed one of our existing
        // inboxes into a new group via that binary's libxmtp; the
        // welcome is on the server but our local sqlite has no record
        // until we sync. If we filtered first, those groups would be
        // dropped (no local membership yet) and `AddPrimaryToExistingGroups`
        // would never run on them — leaving this run's primary out of
        // groups it should join, which downstream rev-leg runs then
        // trip on.
        {
            let mut clients: Vec<Arc<DbgClient>> = Vec::new();
            clients.push(primary.clone());
            for c in existing_clients.values() {
                clients.push(c.clone());
            }
            let syncs = clients.iter().map(|c| async move {
                if let Err(e) = c.sync_welcomes().await {
                    tracing::warn!(
                        target: "healthcheck",
                        inbox = c.inbox_id(),
                        error = %e,
                        "sync_welcomes pre-filter fanout failed",
                    );
                }
            });
            futures::future::join_all(syncs).await;
        }

        // 4. Existing groups from redb. Filter out DMs: rev-leg's fresh
        // primary isn't a DM member, so DM-typed entries would fail every
        // op. Probe conversation_type via any existing_client that has
        // the group locally; if none does, drop it (rev primary couldn't
        // operate on it anyway).
        let raw_group_ids: Vec<GroupId> = group_store
            .load(net_key)?
            .map(|iter| {
                iter.map(|g| GroupId::from(g.value().id.as_slice()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut existing_groups = Vec::with_capacity(raw_group_ids.len());
        for gid in raw_group_ids {
            let mut keep = false;
            for client in existing_clients.values() {
                let Ok(group) = client.group(&gid.to_vec()) else {
                    continue;
                };
                let Ok(md) = group.metadata().await else {
                    continue;
                };
                keep = matches!(md.conversation_type, xmtp_db::group::ConversationType::Group);
                break;
            }
            if keep {
                existing_groups.push(gid);
            }
        }

        tracing::info!(
            target: "healthcheck",
            existing_identities = existing_clients.len(),
            other_identities = other_identities.len(),
            existing_groups = existing_groups.len(),
            bootstrap = bootstrap_clients.len(),
            "health context bootstrapped"
        );

        let ctx = Self {
            network,
            primary,
            bootstrap_clients,
            existing_clients,
            other_identities,
            existing_groups,
            new_groups: Vec::new(),
            xdbg_version: crate::get_version(),
            network_key: net_key,
        };
        Ok(ctx)
    }

    /// Persist a newly-created group to redb's `GroupStore` so subsequent
    /// healthcheck runs see it as an existing group. Panics on redb
    /// failure — that indicates an xdbg state-directory issue.
    pub fn persist_new_group(&self, id: [u8; 16], created_by: InboxId, members: Vec<InboxId>) {
        let group_store: GroupStore<'static> = redb_or_panic("persist_new_group").into();
        let group = Group {
            id,
            created_by,
            member_size: members.len() as u32,
            members,
            version_string: Group::pack_current_version().expect("stamp xdbg version on Group"),
        };
        group_store
            .set(group, u64::from(&self.network))
            .expect("redb GroupStore::set failed");
    }

    /// Replace a persisted group's member list.
    pub fn update_group_members(&self, id: [u8; 16], members: Vec<InboxId>) {
        let group_store: GroupStore<'static> = redb_or_panic("update_group_members").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
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
            version_string: Group::pack_current_version().expect("stamp xdbg version on Group"),
        };
        group_store
            .set(group, net_key)
            .expect("redb GroupStore::set failed");
    }

    /// Look up a persisted group's current members, if it's recorded.
    pub fn persisted_members(&self, id: [u8; 16]) -> Vec<InboxId> {
        let group_store: GroupStore<'static> = redb_or_panic("persisted_members").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
        group_store
            .get(key)
            .expect("redb GroupStore::get failed")
            .map(|g| g.members)
            .unwrap_or_default()
    }

    /// Read the persisted `created_by` for a group. Returns `None` when no
    /// row exists or `created_by` is the all-zero placeholder written by
    /// `update_group_members` for groups created before persistence was
    /// added.
    pub fn persisted_creator(&self, id: [u8; 16]) -> Option<InboxId> {
        let group_store: GroupStore<'static> = redb_or_panic("persisted_creator").into();
        let net_key = u64::from(&self.network);
        let key = crate::app::store::NetworkKey::new(net_key, id);
        group_store
            .get(key)
            .expect("redb GroupStore::get failed")
            .map(|g| g.created_by)
            .filter(|c| c != &[0u8; 32])
    }

    /// Iterate every client involved in this run: primary + bootstrap +
    /// existing. Bootstrap clients are also in `existing_clients` (same
    /// `Arc`); de-duplicate by inbox_id.
    pub fn all_clients(&self) -> Vec<Arc<DbgClient>> {
        let mut seen: HashMap<InboxId, Arc<DbgClient>> = HashMap::new();
        seen.insert(inbox_id_bytes(&self.primary), self.primary.clone());
        for c in &self.bootstrap_clients {
            seen.insert(inbox_id_bytes(c), c.clone());
        }
        for (id, c) in &self.existing_clients {
            seen.insert(*id, c.clone());
        }
        seen.into_values().collect()
    }

    /// Create a fresh, registered single-use identity. Not persisted to
    /// redb and not added to `existing_clients` — exists only for the
    /// caller's scope. Used by destructive ops (`LeaveGroup`,
    /// `RemoveMember`) so they're self-contained and don't leave
    /// removed-but-known members lying around across runs.
    pub async fn create_transient(&self) -> Result<Arc<DbgClient>> {
        let wallet = app::generate_wallet();
        let client = app::new_unregistered_client(&self.network, Some(&wallet)).await?;
        app::register_client(&client, wallet.into_alloy()).await?;
        Ok(Arc::new(client))
    }

    /// Concurrently sync welcomes on every client involved in the run.
    /// Failures are logged but never propagated — `sync_welcomes` is
    /// best-effort plumbing, not a validation step.
    pub async fn sync_welcomes_fanout(&self, label: &'static str) {
        let mut clients: Vec<Arc<DbgClient>> = Vec::new();
        clients.push(self.primary.clone());
        for c in self.existing_clients.values() {
            clients.push(c.clone());
        }
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
    /// `xdbg_version` field is sourced from `self` so every row carries
    /// this run's identity. Panics on redb failure.
    pub fn record_message(
        &self,
        group_id: [u8; 16],
        message_id: [u8; 32],
        sender_inbox_id: InboxId,
    ) {
        let sent_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let store: MessageStore<'static> = redb_or_panic("record_message").into();
        let msg = Message {
            id: message_id,
            group_id,
            sender_inbox_id,
            sent_at_ns,
            xdbg_version: self.xdbg_version.clone(),
        };
        store
            .set(msg, self.network_key)
            .expect("redb MessageStore::set failed");
    }

    /// Load every recorded message for this network and bucket by
    /// `group_id`. Single scan; the validator calls this once and reads
    /// per-group sub-vecs out of the map.
    pub fn recorded_messages_by_group(&self) -> HashMap<[u8; 16], Vec<Message>> {
        let store: MessageStore<'static> = redb_or_panic("recorded_messages_by_group").into();
        let Some(iter) = store
            .load(self.network_key)
            .expect("redb MessageStore::load failed")
        else {
            return HashMap::new();
        };
        let mut out: HashMap<[u8; 16], Vec<Message>> = HashMap::new();
        for guard in iter {
            let msg = guard.value();
            out.entry(msg.group_id).or_default().push(msg);
        }
        out
    }
}

/// Decode the client's hex `inbox_id` into the 32-byte form used as the
/// `HashMap` key. The hex form is guaranteed valid by libxmtp.
fn inbox_id_bytes(client: &DbgClient) -> InboxId {
    inbox_id_to_bytes(client.inbox_id())
}

/// Decode a libxmtp hex inbox_id into the 32-byte form xdbg's redb uses.
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
