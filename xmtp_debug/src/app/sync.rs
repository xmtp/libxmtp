//! `xdbg sync` — bring loaded identities' view of the network up to
//! date in libxmtp's SQLite, then reconcile redb against what libxmtp
//! now knows.
//!
//! Honors `--strict-versioning`: walks all identities visible at the
//! current network-key partition. No implicit behavior.

use crate::app::store::{Database, DeriveKey, GroupStore, IdentityStore, MessageStore};
use crate::app::{self, App};
use crate::args::{self, SyncOpts};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::group_message::GroupMessageKind;
use color_eyre::eyre::{self, Result};
use std::time::Instant;
use tracing::warn;

/// `xdbg_version` stamped on messages discovered via sync.
pub const ORPHAN_XDBG_VERSION: &str = "sync";

pub struct Sync {
    #[allow(dead_code)]
    opts: SyncOpts,
    network: args::BackendOpts,
    strict_versioning: bool,
}

impl Sync {
    pub fn new(opts: SyncOpts, network: args::BackendOpts, strict_versioning: bool) -> Self {
        Self {
            opts,
            network,
            strict_versioning,
        }
    }

    pub async fn run(self) -> Result<()> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.clone().into();
        let message_store: MessageStore<'static> = redb.into();

        let _net_key = u64::from(&self.network);
        let clients = app::load_all_identities(&id_store, &self.network, self.strict_versioning)?;

        let mut success = 0usize;
        let mut total_new_groups = 0usize;
        let mut total_updated_groups = 0usize;
        let mut total_orphan_messages = 0usize;
        let mut total_errors = 0usize;

        for (inbox_id, mutex) in clients.iter() {
            let client = mutex.lock().await;
            let start = Instant::now();
            match Self::sync_one(&client, &group_store, &message_store, _net_key).await {
                Ok(stats) => {
                    success += 1;
                    total_new_groups += stats.new_groups;
                    total_updated_groups += stats.updated_groups;
                    total_orphan_messages += stats.orphan_messages;
                    println!(
                        "inbox={}  ({} new group{}, {} updated, {} orphan messages, {:?})",
                        hex::encode(inbox_id),
                        stats.new_groups,
                        if stats.new_groups == 1 { "" } else { "s" },
                        stats.updated_groups,
                        stats.orphan_messages,
                        start.elapsed()
                    );
                }
                Err(e) => {
                    total_errors += 1;
                    warn!(
                        inbox_id = hex::encode(inbox_id),
                        error = %e,
                        "sync failed for identity"
                    );
                    println!("inbox={}  ERROR: {e:#}", hex::encode(inbox_id));
                }
            }
        }

        println!(
            "\nsync summary: {success} identities synced, {total_new_groups} new groups, \
             {total_updated_groups} updated, {total_orphan_messages} orphan messages, \
             {total_errors} errors"
        );

        if success == 0 {
            eyre::bail!("sync: zero identities synced successfully");
        }
        Ok(())
    }

    async fn sync_one(
        client: &crate::DbgClient,
        group_store: &GroupStore<'static>,
        message_store: &MessageStore<'static>,
        net_key: u64,
    ) -> Result<SyncStats> {
        use xmtp_db::prelude::QueryGroupMessage;

        let mut stats = SyncStats::default();

        // sync_all_welcomes_and_groups handles welcome pulling and
        // per-group commit syncing in one call, with internal
        // concurrency + skip-if-already-synced filtering.
        let _summary = client
            .sync_all_welcomes_and_groups(None)
            .await
            .map_err(color_eyre::eyre::Report::from)?;

        // Enumerate groups this identity knows about for redb
        // reconciliation. libxmtp's state is fully synced at this point.
        let groups = client
            .find_groups(Default::default())
            .map_err(color_eyre::eyre::Report::from)?;

        for group in groups {
            let gid_bytes: [u8; 16] = match group.group_id.as_slice().try_into() {
                Ok(b) => b,
                Err(_) => {
                    tracing::warn!(
                        target: "xdbg.sync",
                        group_id_len = group.group_id.len(),
                        "group_id is not 16 bytes; skipping"
                    );
                    continue;
                }
            };
            let gid_display = xmtp_proto::types::GroupId::from(group.group_id.as_slice());

            // 4. Reconcile GroupStore membership.
            let live_members: Vec<[u8; 32]> = match group.members().await {
                Ok(members) => members
                    .into_iter()
                    .map(|m| crate::app::health::inbox_id_to_bytes(&m.inbox_id))
                    .collect(),
                Err(e) => {
                    tracing::warn!(
                        target: "xdbg.sync",
                        inbox = client.inbox_id(),
                        group = %gid_display,
                        error = %e,
                        "group.members failed; skipping"
                    );
                    continue;
                }
            };

            let group_store_key = crate::app::store::NetworkKey::new(net_key, gid_bytes);
            let persisted = group_store.get(group_store_key)?;
            let creator_bytes = persisted
                .as_ref()
                .map(|g| g.created_by)
                .unwrap_or([0u8; 32]);

            match persisted {
                None => {
                    let new_group = crate::app::types::Group {
                        id: gid_bytes,
                        created_by: creator_bytes,
                        member_size: live_members.len() as u32,
                        members: live_members.clone(),
                        version_string: crate::app::types::Group::pack_current_version()?,
                    };
                    group_store.set(new_group, net_key)?;
                    stats.new_groups += 1;
                    tracing::info!(
                        target: "xdbg.sync",
                        group = %gid_display,
                        members = live_members.len(),
                        "imported new group into GroupStore"
                    );
                }
                Some(existing) => {
                    if existing.members != live_members {
                        let updated = crate::app::types::Group {
                            id: existing.id,
                            created_by: existing.created_by,
                            member_size: live_members.len() as u32,
                            members: live_members.clone(),
                            version_string: existing.version_string,
                        };
                        group_store.set(updated, net_key)?;
                        stats.updated_groups += 1;
                        tracing::info!(
                            target: "xdbg.sync",
                            group = %gid_display,
                            members = live_members.len(),
                            "updated GroupStore membership"
                        );
                    }
                }
            }

            // 5. Discover orphan messages in libxmtp's SQLite for this group.
            let db = client.db();
            let filter = MsgQueryArgs::builder().kind(GroupMessageKind::Application).build().expect("must be infallible");
            let msgs = match db.get_group_messages(
                group.group_id.as_slice(),
                &filter,
            ) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        target: "xdbg.sync",
                        inbox = client.inbox_id(),
                        group = %gid_display,
                        error = %e,
                        "get_group_messages failed; skipping orphan import"
                    );
                    continue;
                }
            };

            for m in msgs {
                let message_id: [u8; 32] = match m.id.as_slice().try_into() {
                    Ok(b) => b,
                    Err(_) => continue, // non-32-byte ids (shouldn't happen but be defensive)
                };

                // Build the composite MessageKey (group_id ++ message_id = 48 bytes)
                // to check redb for existing record.
                let candidate = crate::app::types::Message {
                    id: message_id,
                    group_id: gid_bytes,
                    sender_inbox_id: [0u8; 32], // placeholder; key only uses group_id + id
                    sent_at_ns: 0,
                    xdbg_version: ORPHAN_XDBG_VERSION.to_string(),
                };
                let msg_key = candidate.key(net_key);

                if message_store.get(msg_key)?.is_none() {
                    let sender_bytes = crate::app::health::inbox_id_to_bytes(&m.sender_inbox_id);
                    message_store.set(
                        crate::app::types::Message {
                            id: message_id,
                            group_id: gid_bytes,
                            sender_inbox_id: sender_bytes,
                            sent_at_ns: m.sent_at_ns,
                            xdbg_version: ORPHAN_XDBG_VERSION.to_string(),
                        },
                        net_key,
                    )?;
                    stats.orphan_messages += 1;
                    tracing::warn!(
                        target: "xdbg.sync",
                        inbox = client.inbox_id(),
                        group = %gid_display,
                        message_id = %hex::encode(message_id),
                        "recorded orphan message via sync — sender did not record it"
                    );
                }
            }
        }

        Ok(stats)
    }
}

#[derive(Default, Debug)]
struct SyncStats {
    new_groups: usize,
    updated_groups: usize,
    orphan_messages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sync` writes orphan messages with this sentinel value so
    /// `NoMissingMessages` can distinguish sender-recorded vs
    /// sync-discovered rows.
    #[test]
    fn orphan_message_sentinels() {
        assert_eq!(ORPHAN_XDBG_VERSION, "sync");
    }
}
