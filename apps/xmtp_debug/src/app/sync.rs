//! `xdbg sync` — bring loaded identities' view of the network up to
//! date in libxmtp's SQLite, then reconcile redb against what libxmtp
//! now knows.
//!
//! Honors `--strict-versioning`: walks all identities visible at the
//! current network-key partition. No implicit behavior.

use crate::app::health::inbox_id_to_bytes;
use crate::app::store::{Database, GroupStore, IdentityStore, MessageStore, NetworkKey};
use crate::app::types::{Group, InboxId, Message};
use crate::app::{self, App};
use crate::args::{self, SyncOpts};
use color_eyre::eyre::{self, Report, Result};
use itertools::Itertools;
use std::time::Instant;
use tracing::warn;
use xmtp_db::prelude::QueryGroupMessage;

pub struct Sync {
    #[allow(dead_code)]
    opts: SyncOpts,
    network: args::BackendOpts,
}

impl Sync {
    pub fn new(opts: SyncOpts, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.clone().into();
        let message_store: MessageStore<'static> = redb.into();

        let net_key = u64::from(&self.network);
        let clients = app::load_all_identities(&id_store, &self.network)?;

        let mut totals = SyncStats::default();
        let mut success = 0usize;
        let mut errors = 0usize;

        for (inbox_id, mutex) in clients.iter() {
            let client = mutex.lock().await;
            let start = Instant::now();
            match Self::sync_one(&client, &group_store, &message_store, net_key).await {
                Ok(stats) => {
                    success += 1;
                    totals += &stats;
                    println!(
                        "inbox={}  ({stats}, {:?})",
                        hex::encode(inbox_id),
                        start.elapsed()
                    );
                }
                Err(e) => {
                    errors += 1;
                    warn!(
                        inbox_id = hex::encode(inbox_id),
                        error = %e,
                        "sync failed for identity"
                    );
                    println!("inbox={}  ERROR: {e:#}", hex::encode(inbox_id));
                }
            }
        }

        println!("\nsync summary: {success} identities synced, {totals}, {errors} errors");

        if success == 0 {
            eyre::bail!("sync: zero identities synced successfully");
        }

        if errors > 0 {
            eyre::bail!("sync failed for 1 or more identities");
        }
        Ok(())
    }

    async fn sync_one(
        client: &crate::DbgClient,
        group_store: &GroupStore<'static>,
        message_store: &MessageStore<'static>,
        net_key: u64,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let summary = client.sync_all_welcomes_and_groups(None).await?;
        stats.skipped_groups = summary.num_eligible.saturating_sub(summary.num_synced);

        let groups = client.find_groups(Default::default())?;

        for group in groups {
            let gid = group.group_id;

            let live_members: Vec<InboxId> = group
                .members()
                .await?
                .into_iter()
                .map(|m| Ok::<_, Report>(inbox_id_to_bytes(&m.inbox_id)))
                .try_collect()?;

            let group_store_key = NetworkKey::new(net_key, gid);
            let persisted = group_store.get(group_store_key)?;

            match persisted {
                None => {
                    let md = group.metadata().await?;
                    let creator_bytes = inbox_id_to_bytes(&md.creator_inbox_id);
                    let member_count = live_members.len();
                    group_store.set(Group::new(gid, creator_bytes, live_members), net_key)?;
                    stats.new_groups += 1;
                    tracing::info!(
                        target: "xdbg.sync",
                        group = %gid,
                        members = member_count,
                        "imported new group into GroupStore"
                    );
                }
                Some(existing) if existing.members != live_members => {
                    let member_count = live_members.len();
                    group_store.set(
                        Group::new(*existing.id(), existing.created_by, live_members),
                        net_key,
                    )?;
                    stats.updated_groups += 1;
                    tracing::info!(
                        target: "xdbg.sync",
                        group = %gid,
                        members = member_count,
                        "updated GroupStore membership"
                    );
                }
                Some(_) => {}
            }

            let db = client.db();
            let msgs = db.get_group_messages(&gid, &Default::default())?;

            for m in msgs {
                let message_id: [u8; 32] =
                    m.id.as_slice()
                        .try_into()
                        .inspect_err(|_| tracing::error!("message id must be 32 bytes"))?;

                let msg_key = Message::redb_key(net_key, gid, message_id);
                if message_store.get(msg_key)?.is_none() {
                    let sender_bytes = inbox_id_to_bytes(&m.sender_inbox_id);
                    message_store.set(
                        Message::new(message_id, gid, sender_bytes, m.sent_at_ns),
                        net_key,
                    )?;
                    stats.orphan_messages += 1;
                    tracing::warn!(
                        target: "xdbg.sync",
                        inbox = client.inbox_id(),
                        group = %gid,
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
    skipped_groups: usize,
}

impl std::fmt::Display for SyncStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plural = if self.new_groups == 1 { "" } else { "s" };
        write!(
            f,
            "{} new group{plural}, {} updated, {} orphan messages, {} skipped",
            self.new_groups, self.updated_groups, self.orphan_messages, self.skipped_groups,
        )
    }
}

impl std::ops::AddAssign<&SyncStats> for SyncStats {
    fn add_assign(&mut self, rhs: &SyncStats) {
        self.new_groups += rhs.new_groups;
        self.updated_groups += rhs.updated_groups;
        self.orphan_messages += rhs.orphan_messages;
        self.skipped_groups += rhs.skipped_groups;
    }
}
