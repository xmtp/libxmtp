//! Health check for all local XMTP clients on the current network.
//!
//! Runs 5 checks per client:
//! 1. No missing messages  — every network message for each group exists locally
//! 2. Can send             — a test message can be sent on at least one active group
//! 3. Can receive          — group messages can be queried from the network
//! 4. No fork              — no group has `is_commit_log_forked == Some(true)`
//! 5. Identity reachable   — the inbox_id is visible on the network

use std::{collections::HashSet, sync::Arc};

use color_eyre::eyre::{Result, eyre};
use xmtp_db::encrypted_store::group::GroupMembershipState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::prelude::QueryGroup;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest,
    get_identity_updates_request::Request as IdentityRequest,
};

use crate::{
    app::{
        App,
        store::{Database, IdentityStore},
    },
    args,
};

pub struct Healthcheck {
    opts: args::HealthcheckOpts,
    network: args::BackendOpts,
    db: Arc<redb::ReadOnlyDatabase>,
}

struct GroupHealthResult {
    group_id: String,
    /// Number of network messages with no matching local record
    missing_message_count: usize,
    can_send: bool,
    can_receive: bool,
    /// `None` = status unknown, `Some(true)` = forked
    is_forked: Option<bool>,
}

struct ClientHealthResult {
    inbox_id: String,
    identity_reachable: bool,
    groups: Vec<GroupHealthResult>,
}

impl ClientHealthResult {
    fn is_healthy(&self) -> bool {
        self.identity_reachable
            && self
                .groups
                .iter()
                .all(|g| g.missing_message_count == 0 && g.can_send && g.can_receive && g.is_forked != Some(true))
    }
}

impl Healthcheck {
    pub fn new(opts: args::HealthcheckOpts, network: args::BackendOpts) -> Result<Self> {
        let db = App::readonly_db()?;
        Ok(Self { opts, network, db })
    }

    pub async fn run(self) -> Result<()> {
        let Healthcheck { opts, network, db } = self;

        let identity_store: IdentityStore = db.clone().into();
        let network_id = u64::from(&network);

        let identities = identity_store
            .load(network_id)?
            .ok_or_else(|| eyre!("no identities in store for this network – try `xdbg generate identity`"))?;

        let mut all_results: Vec<ClientHealthResult> = Vec::new();
        let mut any_unhealthy = false;

        for identity_guard in identities {
            let identity = identity_guard.value();
            let inbox_id_hex = hex::encode(identity.inbox_id);

            info!(inbox_id = %inbox_id_hex, "checking client");

            let client = match crate::app::client_from_identity(&identity, &network) {
                Ok(c) => c,
                Err(e) => {
                    error!(inbox_id = %inbox_id_hex, error = %e, "failed to load client, skipping");
                    if opts.fail_fast {
                        return Err(e);
                    }
                    continue;
                }
            };

            // ── Check 5: identity reachable ──────────────────────────────────
            let api_client = network.connect()?;
            let identity_reachable = match api_client
                .get_identity_updates_v2(GetIdentityUpdatesRequest {
                    requests: vec![IdentityRequest {
                        inbox_id: inbox_id_hex.clone(),
                        sequence_id: 0,
                    }],
                })
                .await
            {
                Ok(resp) => {
                    let reachable = resp.responses.iter().any(|r| !r.updates.is_empty());
                    if !reachable {
                        warn!(inbox_id = %inbox_id_hex, "identity has no updates on network (not reachable)");
                    }
                    reachable
                }
                Err(e) => {
                    error!(inbox_id = %inbox_id_hex, error = %e, "failed to query identity updates");
                    false
                }
            };

            // ── Per-group checks ─────────────────────────────────────────────
            let groups = client.find_groups(GroupQueryArgs::default())?;
            let mut group_results: Vec<GroupHealthResult> = Vec::new();

            for group in &groups {
                let group_id_hex = hex::encode(&group.group_id);

                // ── Check 1: missing messages ─────────────────────────────
                let network_messages = match api_client
                    .query_group_messages(group.group_id.clone().into())
                    .await
                {
                    Ok(msgs) => msgs,
                    Err(e) => {
                        error!(group_id = %group_id_hex, error = %e, "failed to query network messages");
                        if opts.fail_fast {
                            return Err(e.into());
                        }
                        vec![]
                    }
                };

                let local_messages = group.find_messages(&Default::default())?;
                let local_cursors: HashSet<(i64, i64)> = local_messages
                    .iter()
                    .map(|m| (m.sequence_id, m.originator_id))
                    .collect();

                let missing_message_count = network_messages
                    .iter()
                    .filter(|nm| {
                        !local_cursors
                            .contains(&(nm.sequence_id() as i64, nm.originator_id() as i64))
                    })
                    .count();

                if missing_message_count > 0 {
                    warn!(
                        group_id = %group_id_hex,
                        missing = missing_message_count,
                        "group has messages on network not found locally"
                    );
                }

                // ── Check 3: can receive ──────────────────────────────────
                let can_receive = match group.sync().await {
                    Ok(_) => true,
                    Err(e) => {
                        warn!(group_id = %group_id_hex, error = %e, "group sync failed");
                        false
                    }
                };

                // ── Check 4: fork status ──────────────────────────────────
                let is_forked = {
                    let conn = client.context.db();
                    conn.get_group_commit_log_forked_status(&group.group_id)
                        .unwrap_or(None)
                };
                if is_forked == Some(true) {
                    warn!(group_id = %group_id_hex, "group commit log is FORKED");
                }

                // ── Check 2: can send (deferred, marked per-group) ────────
                // We attempt send on active groups below; initialise to false here.
                group_results.push(GroupHealthResult {
                    group_id: group_id_hex,
                    missing_message_count,
                    can_send: false, // filled in below
                    can_receive,
                    is_forked,
                });
            }

            // ── Check 2: can send (try each active group until one succeeds) ─
            let active_groups: Vec<_> = groups
                .iter()
                .zip(group_results.iter_mut())
                .filter(|(g, _)| {
                    matches!(
                        g.membership_state(),
                        Ok(GroupMembershipState::Allowed) | Ok(GroupMembershipState::PendingRemove)
                    )
                })
                .collect();

            for (group, result) in active_groups {
                match group
                    .send_message(b"xdbg-healthcheck", SendMessageOpts::default())
                    .await
                {
                    Ok(_) => {
                        result.can_send = true;
                        break;
                    }
                    Err(e) => {
                        warn!(
                            group_id = %result.group_id,
                            error = %e,
                            "send_message failed on group"
                        );
                    }
                }
            }

            let client_result = ClientHealthResult {
                inbox_id: inbox_id_hex.clone(),
                identity_reachable,
                groups: group_results,
            };

            print_client_result(&client_result);

            if !client_result.is_healthy() {
                any_unhealthy = true;
                if opts.fail_fast {
                    return Err(eyre!("healthcheck FAILED for client {}", inbox_id_hex));
                }
            }

            all_results.push(client_result);
        }

        // ── Summary ──────────────────────────────────────────────────────────
        println!();
        println!("=== Healthcheck Summary ===");
        println!(
            "Checked {} client(s)",
            all_results.len()
        );

        let healthy = all_results.iter().filter(|r| r.is_healthy()).count();
        let unhealthy = all_results.len() - healthy;
        println!("  healthy:   {healthy}");
        println!("  unhealthy: {unhealthy}");

        if any_unhealthy {
            Err(eyre!("healthcheck FAILED: {unhealthy} client(s) are unhealthy"))
        } else {
            info!("All clients are healthy");
            Ok(())
        }
    }
}

fn print_client_result(r: &ClientHealthResult) {
    let status = if r.is_healthy() { "HEALTHY" } else { "UNHEALTHY" };
    println!();
    println!("── client {} [{status}]", r.inbox_id);
    println!("   identity reachable : {}", r.identity_reachable);
    println!("   groups checked     : {}", r.groups.len());

    for g in &r.groups {
        let fork_str = match g.is_forked {
            Some(true) => "FORKED",
            Some(false) => "ok",
            None => "unknown",
        };
        println!("   ├─ group {}", g.group_id);
        println!("      missing messages : {}", g.missing_message_count);
        println!("      can send         : {}", g.can_send);
        println!("      can receive      : {}", g.can_receive);
        println!("      fork status      : {fork_str}");
    }
}
