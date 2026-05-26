//! `xdbg sync` — bring loaded identities' view of the network up to
//! date in libxmtp's SQLite, then reconcile redb against what libxmtp
//! now knows.
//!
//! Honors `--strict-versioning`: walks all identities visible at the
//! current network-key partition. No implicit behavior.

use crate::app::health::inbox_id_to_bytes;
use crate::app::store::{Database, DmStore, GroupStore, IdentityStore, MessageStore};
use crate::app::types::{Dm, DmId, Group, InboxId, Message};
use crate::app::{self, App};
use crate::args::SyncOpts;
use color_eyre::eyre::{self, Report, Result};
use itertools::Itertools;
use std::time::Instant;
use tracing::warn;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_db::prelude::QueryGroupMessage;

pub struct Sync {
    #[allow(dead_code)]
    opts: &'static SyncOpts,
}

impl Sync {
    pub fn new(opts: &'static SyncOpts) -> Self {
        Self { opts }
    }

    pub async fn run(self) -> Result<()> {
        let redb = App::db()?;
        let id_store: IdentityStore<'static> = redb.clone().into();
        let group_store: GroupStore<'static> = redb.clone().into();
        let dm_store: DmStore<'static> = redb.clone().into();
        let message_store: MessageStore<'static> = redb.into();

        let clients = app::load_all_identities(&id_store)?;

        let mut totals = SyncStats::default();
        let mut success = 0usize;
        let mut errors = 0usize;

        for (inbox_id, mutex) in clients.iter() {
            let client = mutex.lock().await;
            let start = Instant::now();
            match Self::sync_one(&client, &group_store, &dm_store, &message_store).await {
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
        dm_store: &DmStore<'static>,
        message_store: &MessageStore<'static>,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let summary = client.sync_all_welcomes_and_groups(None).await?;
        stats.skipped_groups = summary.num_eligible.saturating_sub(summary.num_synced);

        let all = client.find_groups(Default::default())?;
        let (groups, dms): (Vec<_>, Vec<_>) = all.into_iter().partition(|g| g.dm_id.is_none());

        for dm in dms {
            let gid = dm.group_id;

            let live_members: Vec<InboxId> = dm
                .members()
                .await?
                .into_iter()
                .map(|m| Ok::<_, Report>(inbox_id_to_bytes(&m.inbox_id)))
                .try_collect()?;

            let dm_id = dm.dm_id.clone().expect("dm id must be some");
            let md = dm.metadata().await?;
            let creator_bytes = inbox_id_to_bytes(&md.creator_inbox_id);
            let other = live_members
                .iter()
                .copied()
                .find(|i| *i != creator_bytes)
                .ok_or_else(|| eyre::eyre!("DM has no inbox distinct from the creator"))?;
            let persisted = dm_store.get(DmId::new(creator_bytes, other).into())?;
            match persisted {
                None => {
                    dm_store.set(Dm::new(creator_bytes, other, gid))?;
                    stats.new_dms += 1;
                    tracing::info!(
                        target: "xdbg.sync",
                        id = %dm_id,
                        "imported new dm into DmStore"
                    );
                }
                Some(_existing) => {
                    tracing::info!(
                        target: "xdbg.sync",
                        id = %dm_id,
                        group = %gid,
                        "dm already persisted; skipping"
                    );
                }
            }
        }

        for group in groups {
            let gid = group.group_id;

            let live_members: Vec<InboxId> = group
                .members()
                .await?
                .into_iter()
                .map(|m| Ok::<_, Report>(inbox_id_to_bytes(&m.inbox_id)))
                .try_collect()?;

            let persisted = group_store.get(gid.into())?;
            match persisted {
                None => {
                    let md = group.metadata().await?;
                    let creator_bytes = inbox_id_to_bytes(&md.creator_inbox_id);
                    let member_count = live_members.len();
                    group_store.set(Group::new(gid, creator_bytes, live_members))?;
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
                    group_store.set(Group::new(
                        *existing.id(),
                        existing.created_by,
                        live_members,
                    ))?;
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
            // Application only — MembershipChange commits fan out
            // per-member at backend-dependent times. Recording them
            // makes NoMissingMessages racy across peers.
            let msgs = db.get_group_messages(
                &gid,
                &MsgQueryArgs::builder()
                    .kind(GroupMessageKind::Application)
                    .build()
                    .expect("MsgQueryArgs builder"),
            )?;

            for m in msgs {
                let message_id: [u8; 32] =
                    m.id.as_slice()
                        .try_into()
                        .inspect_err(|_| tracing::error!("message id must be 32 bytes"))?;

                let msg_key = Message::redb_key(gid, message_id);
                if message_store.get(msg_key)?.is_none() {
                    let sender_bytes = inbox_id_to_bytes(&m.sender_inbox_id);
                    message_store.set(Message::new(message_id, gid, sender_bytes, m.sent_at_ns))?;
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
    new_dms: usize,
    updated_groups: usize,
    orphan_messages: usize,
    skipped_groups: usize,
}

impl std::fmt::Display for SyncStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plural_g = if self.new_groups == 1 { "" } else { "s" };
        let plural_d = if self.new_dms == 1 { "" } else { "s" };
        write!(
            f,
            "{} new group{plural_g}, {} new dm{plural_d}, {} updated, {} orphan messages, {} skipped",
            self.new_groups,
            self.new_dms,
            self.updated_groups,
            self.orphan_messages,
            self.skipped_groups,
        )
    }
}

impl std::ops::AddAssign<&SyncStats> for SyncStats {
    fn add_assign(&mut self, rhs: &SyncStats) {
        self.new_groups += rhs.new_groups;
        self.new_dms += rhs.new_dms;
        self.updated_groups += rhs.updated_groups;
        self.orphan_messages += rhs.orphan_messages;
        self.skipped_groups += rhs.skipped_groups;
    }
}
