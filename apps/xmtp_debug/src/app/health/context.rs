//! Shared state for the health-check run.

use crate::DbgClient;
use crate::app::health::conditions::Conditions;
use crate::app::health::result::OpResult;
use crate::app::store::{Database, DeriveKey, DmStore, GroupStore, IdentityStore, MessageStore};
use crate::app::types::{Dm, Group, Identity, InboxId, Message};
use crate::app::{self, App, client_from_identity};
use color_eyre::eyre::{Result, ensure, eyre};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use xmtp_proto::types::GroupId;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct HealthClient {
    identity: Identity,
}

impl HealthClient {
    pub fn new(identity: Identity) -> Self {
        Self { identity }
    }

    /// Raw 32-byte inbox id for this client.
    pub fn inbox_id_bytes(&self) -> InboxId {
        self.identity.inbox_id
    }

    /// Hex-encoded inbox id matching the libxmtp `String` representation.
    pub fn inbox_id_hex(&self) -> String {
        hex::encode(self.identity.inbox_id)
    }
    /// Materialize a fully-functional `DbgClient` from this identity by
    /// reopening its sqlite + reconnecting to the network. Synchronous and
    /// cheap (no network round-trip; just a `client_from_identity` call).
    pub fn realize(&self, store: &IdentityStore<'static>) -> Result<DbgClient> {
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

    pub id_store: IdentityStore<'static>,

    group_store: GroupStore<'static>,
    msg_store: MessageStore<'static>,
    dm_store: DmStore<'static>,

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
        let group_store: GroupStore<'static> = redb.clone().into();
        let dm_store: DmStore<'static> = redb.clone().into();
        let msg_store: MessageStore<'static> = redb.into();

        // 1. Existing identities. Walk the full id_store once.
        //    Under `--strict-versioning`, split by version_hash: only
        //    current-version identities go into `existing` (we'll
        //    realize them as `DbgClient`s), other-version identities
        //    go into `other` (member-add ops still include them by
        //    inbox_id but we never open their sqlite). Without strict
        //    versioning, version partitioning isn't meaningful — load
        //    every identity into `existing` so every group on disk is
        //    reachable through some realizable client, and leave
        //    `other` empty.
        let current_vh = App::current_version_hash();
        let (existing, other) = if App::strict_versioning() {
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
            (existing, other)
        } else {
            let existing = if let Some(ids) = id_store.load()? {
                ids.map(|v| HealthClient::new(v.value()))
                    .collect::<HashSet<_>>()
            } else {
                HashSet::new()
            };
            (existing, HashSet::new())
        };

        let empty = other.is_empty() && existing.is_empty();
        if read_only && empty {
            tracing::info!("no identities to check.");
            std::process::exit(0);
        }

        if empty {
            conditions.insert(Conditions::BOOTSTRAP);
        }

        // Primary selection:
        //   - read_only: must reuse an existing identity (no writes,
        //     CreateIdentity won't run). The empty-store case already
        //     exited above.
        //   - writable runs: leave primary as a local-only placeholder.
        //     `CreateIdentity` runs first (no deps) and overwrites
        //     `ctx.primary` with a freshly-registered on-network identity.
        //     No op realizes the placeholder before `CreateIdentity` runs.
        let primary = if read_only {
            existing.iter().next().cloned().ok_or_else(|| {
                eyre!("read_only healthcheck requires at least one existing client")
            })?
        } else {
            HealthClient::new(Identity::generate()?)
        };

        let existing_groups: Vec<GroupId> = group_store
            .load()?
            .map(|iter| iter.map(|g| g.value().id()).collect())
            .unwrap_or_default();

        tracing::info!(
            target: "healthcheck",
            existing_identities = existing.len(),
            other_identities = other.len(),
            existing_groups = existing_groups.len(),
            bootstrap_needed = empty,
            "health context bootstrapped"
        );

        let ctx = Self {
            primary,
            existing_clients: existing,
            other_identities: other,
            existing_groups,
            new_groups: Vec::new(),
            group_store,
            id_store,
            dm_store,
            msg_store,
        };

        // Drain welcomes left server-side by prior cross-version runs;
        // without this the first `client.group(<gid>)` hits "not found".
        // Skipped on first-run-empty: no identities to sync, and the
        // `BootstrapIdentities` op will register fresh ones. On
        // writable runs we sync only existing peers (primary is a
        // local-only placeholder until `CreateIdentity` registers it).
        if !empty {
            if read_only {
                ctx.sync_welcomes_fanout("bootstrap").await?;
            } else {
                ctx.sync_welcomes_existing("bootstrap").await?;
            }
        }
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
        self.group_store
            .set(Group::new(*group_id, created_by, members))
            .expect("redb GroupStore::set failed");
    }

    /// Persist a newly-created DM to the redb `DmStore`. DMs live in
    /// their own table — they're keyed by the (creator, peer) inbox
    /// pair, not by group id, so cross-version runs can find the same
    /// DM regardless of which version created the underlying MLS group.
    ///
    /// `members` must contain exactly two inboxes: `created_by` and the
    /// peer. Panics on shape violations — DM creation is an op-level
    /// invariant and a wrong shape means a deeper bug.
    pub fn persist_new_dm(&self, group_id: &GroupId, created_by: InboxId, members: Vec<InboxId>) {
        let peer = members
            .iter()
            .copied()
            .find(|m| *m != created_by)
            .expect("DM must contain a peer distinct from the creator");
        self.dm_store
            .set(Dm::new(created_by, peer, *group_id))
            .expect("redb DmStore::set failed");
    }

    /// Replace a persisted group's member list. Used by membership ops
    /// (`AddMembersToNewGroup`, `AddPrimaryToExistingGroups`,
    /// `RemoveMember`, `LeaveGroup`) so redb's view stays consistent with
    /// the MLS-level state across runs.
    ///
    /// Panics on redb failure or non-16-byte `group_id`.
    pub fn update_group_members(&self, group_id: &GroupId, members: Vec<InboxId>) {
        // to zero — the field is informational, not load-bearing.
        let created_by = self
            .group_store
            .get(group_id.into())
            .expect("redb GroupStore::get failed")
            .map(|g| g.created_by)
            .unwrap_or([0u8; 32]);
        self.group_store
            .set(Group::new(*group_id, created_by, members))
            .expect("redb GroupStore::set failed");
    }

    /// Look up a persisted group's current members, if it's recorded.
    /// Returns an empty vec if the group is not in redb.
    pub fn persisted_members(&self, group_id: &GroupId) -> Vec<InboxId> {
        self.group_store
            .get(group_id.into())
            .expect("redb GroupStore::get failed")
            .map(|g| g.members)
            .unwrap_or_default()
    }

    /// Every persisted client involved in this run: primary + every
    /// entry in `existing_clients`. Run-scoped throwaway transients
    /// created by ops are NOT included — they're op-local. Under
    /// `read_only`, primary is one of `existing_clients` — deduped
    /// here so it isn't materialized twice.
    ///
    /// Materializes each `HealthClient` via `client_from_identity` (sync,
    /// sqlite-only). Errors propagate to the caller.
    pub fn all_clients(&self) -> Result<Vec<DbgClient>> {
        let primary_inbox = self.primary.inbox_id_bytes();
        let mut out = Vec::with_capacity(self.existing_clients.len() + 1);
        out.push(self.primary.realize(&self.id_store)?);
        for hc in self.existing_clients.iter() {
            if hc.inbox_id_bytes() == primary_inbox {
                continue;
            }
            out.push(hc.realize(&self.id_store)?);
        }
        Ok(out)
    }

    /// Every group this run cares about: existing (loaded from redb)
    /// followed by new groups created by ops in this run.
    pub fn all_groups(&self) -> impl Iterator<Item = &GroupId> {
        self.existing_groups.iter().chain(self.new_groups.iter())
    }

    /// Run `f` once per group in [`all_groups`](Self::all_groups) with the
    /// primary realized once. `Ok(())` → Pass row, `Err` → Fail row,
    /// per group. Pre-loop failures (primary materialization) collapse
    /// to a single Fail row.
    pub async fn for_each_group<F, Fut>(&self, op_name: &'static str, f: F) -> Vec<OpResult>
    where
        F: Fn(DbgClient, GroupId) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let primary = match self.primary() {
            Ok(p) => p,
            Err(e) => return vec![OpResult::fail(op_name, None, e)],
        };
        let mut out = Vec::new();
        for gid in self.all_groups().copied().collect::<Vec<_>>() {
            let start = Instant::now();
            let res = f(primary.clone(), gid).await;
            out.push(OpResult::from_result(
                op_name,
                Some(format!("{gid}")),
                start,
                res,
            ));
        }
        out
    }

    /// Run `f` once per `(client, group)` pair across `all_clients` ×
    /// `all_groups`. Clients realized once up front. Pre-loop failures
    /// collapse to a single Fail row.
    pub async fn for_each_client_group<F, Fut>(&self, op_name: &'static str, f: F) -> Vec<OpResult>
    where
        F: Fn(DbgClient, GroupId) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let clients = match self.all_clients() {
            Ok(cs) => cs,
            Err(e) => return vec![OpResult::fail(op_name, None, e)],
        };
        let groups: Vec<GroupId> = self.all_groups().copied().collect();
        let mut out = Vec::new();
        for client in &clients {
            for gid in &groups {
                let start = Instant::now();
                let target = format!("inbox={} group={gid}", client.inbox_id());
                let res = f(client.clone(), *gid).await;
                out.push(OpResult::from_result(op_name, Some(target), start, res));
            }
        }
        out
    }

    /// Run `f` once per client in [`all_clients`](Self::all_clients).
    /// Each client realized once up front. Pre-loop failures collapse to
    /// a single Fail row.
    pub async fn for_each_client<F, Fut>(&self, op_name: &'static str, f: F) -> Vec<OpResult>
    where
        F: Fn(DbgClient) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let clients = match self.all_clients() {
            Ok(cs) => cs,
            Err(e) => return vec![OpResult::fail(op_name, None, e)],
        };
        let mut out = Vec::new();
        for client in clients {
            let start = Instant::now();
            let target = client.inbox_id().to_string();
            let res = f(client).await;
            out.push(OpResult::from_result(op_name, Some(target), start, res));
        }
        out
    }

    /// Run `f` once per client in `existing_clients` (skipping primary).
    /// Each client realized lazily — a single materialize failure short-
    /// circuits the loop into a Fail row for that client and continues
    /// with the next one.
    /// Run `f` once per existing peer (skipping the primary identity).
    /// `f` returns its own `Vec<OpResult>` so callers that emit multiple
    /// rows per peer (e.g. `CreateDm` with forward + reverse direction
    /// rows) fit naturally. Closure receives `&self` so it can call
    /// helpers; uses `BoxFuture` for the same lifetime reasons as
    /// [`for_each_existing_group`](Self::for_each_existing_group).
    pub async fn for_each_existing_client<F>(&self, f: F) -> Vec<OpResult>
    where
        F: for<'a> Fn(&'a Self, &'a HealthClient) -> futures::future::BoxFuture<'a, Vec<OpResult>>,
    {
        let primary_bytes = self.primary.inbox_id_bytes();
        let mut out = Vec::new();
        for hc in self
            .existing_clients
            .iter()
            .filter(|hc| hc.inbox_id_bytes() != primary_bytes)
        {
            out.extend(f(self, hc).await);
        }
        out
    }

    /// Run `f` once per group in `existing_groups` (skipping `new_groups`).
    /// Used by ops that operate on groups the previous run created.
    /// `f` receives `&self` so it can call helpers like
    /// [`pick_super_admin`](Self::pick_super_admin) or
    /// [`persisted_members`](Self::persisted_members).
    ///
    /// The closure returns a `BoxFuture` so it can borrow `&self` for
    /// the duration of the awaited work. Direct `async move` closures
    /// can't express "future borrows the argument for its full lifetime"
    /// in stable Rust today.
    pub async fn for_each_existing_group<F>(&self, op_name: &'static str, f: F) -> Vec<OpResult>
    where
        F: for<'a> Fn(&'a Self, GroupId) -> futures::future::BoxFuture<'a, Result<()>>,
    {
        let mut out = Vec::new();
        for gid in self.existing_groups.iter().copied() {
            let start = Instant::now();
            let res = f(self, gid).await;
            out.push(OpResult::from_result(
                op_name,
                Some(format!("{gid}")),
                start,
                res,
            ));
        }
        out
    }

    /// Run `f` against the first group in `new_groups`. Single-row
    /// outcome (the op produces one new group at a time). Pre-checks
    /// for "no new group" and primary materialization collapse to a
    /// single Fail row. The closure returns a `BoxFuture` so it can
    /// borrow `&mut self` for the duration of the awaited work.
    pub async fn for_new_group<F>(&mut self, op_name: &'static str, f: F) -> Vec<OpResult>
    where
        F: for<'a> FnOnce(
            &'a mut Self,
            DbgClient,
            GroupId,
        ) -> futures::future::BoxFuture<'a, Result<()>>,
    {
        let Some(gid) = self.new_groups.first().copied() else {
            return vec![OpResult::fail(
                op_name,
                None,
                eyre!("no new group; CreateGroup must run first"),
            )];
        };
        let primary = match self.primary() {
            Ok(p) => p,
            Err(e) => return vec![OpResult::fail(op_name, Some(format!("{gid}")), e)],
        };
        let start = Instant::now();
        let res = f(self, primary, gid).await;
        vec![OpResult::from_result(
            op_name,
            Some(format!("{gid}")),
            start,
            res,
        )]
    }

    /// Pick an active adder for a membership change; prefer super-admin so
    /// `AddSuper` is also issuable, else any active member (under
    /// `--strict-versioning` the creator may live in another partition).
    pub fn pick_super_admin(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<xmtp_mls::groups::MlsGroup<crate::MlsContext>>> {
        let mut candidates: Vec<(DbgClient, xmtp_mls::groups::MlsGroup<crate::MlsContext>)> =
            Vec::new();
        for hc in self.existing_clients.iter() {
            let client = hc.realize(&self.id_store)?;
            let Ok(group) = client.group(group_id) else {
                continue;
            };
            if !group.is_active().unwrap_or(false) {
                continue;
            }
            candidates.push((client, group));
        }
        Ok(candidates
            .iter()
            .find(|(c, g)| g.is_super_admin(c.inbox_id().to_string()).unwrap_or(false))
            .or_else(|| candidates.first())
            .map(|(_, g)| g.clone()))
    }

    /// Best-effort concurrent welcome sync across all run clients;
    /// per-client failures surface as a warning rather than aborting
    /// the fanout — this is plumbing, not a validation step. A failure
    /// to *materialize* a client (sqlite or version mismatch) is
    /// propagated.
    pub async fn sync_welcomes_fanout(&self, label: &'static str) -> Result<()> {
        let clients = self.all_clients()?;
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
        Ok(())
    }

    /// Like [`sync_welcomes_fanout`](Self::sync_welcomes_fanout) but
    /// excludes `ctx.primary`. Used during `bootstrap()` on writable
    /// runs where primary is still a placeholder.
    pub async fn sync_welcomes_existing(&self, label: &'static str) -> Result<()> {
        let clients: Vec<DbgClient> = self
            .existing_clients
            .iter()
            .map(|hc| hc.realize(&self.id_store))
            .collect::<Result<_>>()?;
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
        Ok(())
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
        let msg = Message::new(
            message_id,
            *group_id,
            inbox_id_to_bytes(sender.inbox_id()),
            sent_at_ns,
        );
        self.msg_store
            .set(msg)
            .expect("redb MessageStore::set failed");
    }

    /// Load every recorded message for this network and bucket by
    /// `group_id`. Single scan; the validator calls this once and reads
    /// per-group sub-vecs out of the map.
    pub fn recorded_messages_by_group(&self) -> HashMap<GroupId, Vec<Message>> {
        let Some(iter) = self
            .msg_store
            .load()
            .expect("redb MessageStore::load failed")
        else {
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
