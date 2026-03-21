//! Health check for all local XMTP clients on the current network.
//!
//! Runs checks per client:
//! 1. No missing messages  — every network message for each group exists locally
//! 2. Can send             — a test message can be sent on at least one active group
//! 3. Can receive          — group messages can be queried from the network
//! 4. No fork              — no group has `maybe_forked` or `is_commit_log_forked`
//! 5. Identity reachable   — the inbox_id is visible on the network

use std::{collections::HashSet, process, sync::Arc};

use color_eyre::eyre::{Result, eyre};
use owo_colors::OwoColorize;
use xmtp_db::encrypted_store::group::GroupMembershipState;
use xmtp_db::group::{GroupQueryArgs, StoredGroup};
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

/// Sequence IDs of messages present on the network but missing locally.
type MissingMsg = (i64, i64); // (sequence_id, originator_id)

struct GroupHealthResult {
    group_id: String,
    missing_messages: Vec<MissingMsg>,
    can_send: bool,
    can_receive: bool,
    /// `maybe_forked` flag from the local DB
    maybe_forked: bool,
    /// Human-readable fork details when `maybe_forked` is set
    fork_details: String,
    /// `None` = status unknown, `Some(true)` = commit log diverged from remote
    is_commit_log_forked: Option<bool>,
}

impl GroupHealthResult {
    fn is_healthy(&self) -> bool {
        self.missing_messages.is_empty()
            && self.can_send
            && self.can_receive
            && !self.maybe_forked
            && self.is_commit_log_forked != Some(true)
    }
}

struct ClientHealthResult {
    inbox_id: String,
    identity_reachable: bool,
    groups: Vec<GroupHealthResult>,
}

impl ClientHealthResult {
    fn is_healthy(&self) -> bool {
        self.identity_reachable && self.groups.iter().all(|g| g.is_healthy())
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
            let conn = client.context.db();
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

                let missing_messages: Vec<MissingMsg> = network_messages
                    .iter()
                    .filter(|nm| {
                        !local_cursors
                            .contains(&(nm.sequence_id() as i64, nm.originator_id() as i64))
                    })
                    .map(|nm| (nm.sequence_id() as i64, nm.originator_id() as i64))
                    .collect();

                if !missing_messages.is_empty() {
                    warn!(
                        group_id = %group_id_hex,
                        missing = missing_messages.len(),
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

                // ── Check 4: fork status (all signals) ────────────────────
                let stored: Option<StoredGroup> = conn.find_group(&group.group_id).unwrap_or(None);
                let maybe_forked = stored.as_ref().map(|s| s.maybe_forked).unwrap_or(false);
                let fork_details = stored
                    .as_ref()
                    .map(|s| s.fork_details.clone())
                    .unwrap_or_default();
                let is_commit_log_forked = conn
                    .get_group_commit_log_forked_status(&group.group_id)
                    .unwrap_or(None);

                if maybe_forked {
                    warn!(group_id = %group_id_hex, details = %fork_details, "group is marked maybe_forked");
                }
                if is_commit_log_forked == Some(true) {
                    warn!(group_id = %group_id_hex, "group commit log is FORKED");
                }

                // ── Check 2: can send (deferred, marked per-group) ────────
                group_results.push(GroupHealthResult {
                    group_id: group_id_hex,
                    missing_messages,
                    can_send: false, // filled in below
                    can_receive,
                    maybe_forked,
                    fork_details,
                    is_commit_log_forked,
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

            log_client_result(&client_result);

            if !client_result.is_healthy() {
                any_unhealthy = true;
                if opts.fail_fast {
                    process::exit(1);
                }
            }

            all_results.push(client_result);
        }

        // ── Summary ──────────────────────────────────────────────────────────
        info!("=== Healthcheck Summary ===");
        info!("Checked {} client(s)", all_results.len());

        let healthy = all_results.iter().filter(|r| r.is_healthy()).count();
        let unhealthy = all_results.len() - healthy;
        info!("  healthy:   {healthy}");
        if unhealthy > 0 {
            info!("  unhealthy: {}", unhealthy.to_string().red().bold());
        } else {
            info!("  unhealthy: {unhealthy}");
        }

        // Print missing sequence IDs per group for any unhealthy clients
        for client in &all_results {
            for group in &client.groups {
                if !group.missing_messages.is_empty() {
                    info!(
                        inbox_id = %client.inbox_id,
                        group_id = %group.group_id,
                        "missing sequence IDs: {:?}",
                        group.missing_messages
                    );
                }
            }
        }

        if any_unhealthy {
            info!("{}", "healthcheck FAILED".red().bold());
            process::exit(1);
        } else {
            info!("All clients are healthy");
            Ok(())
        }
    }
}

fn bool_str(v: bool, healthy_when_true: bool) -> String {
    if v == healthy_when_true {
        v.to_string().green().bold().to_string()
    } else {
        v.to_string().red().bold().to_string()
    }
}

fn log_client_result(r: &ClientHealthResult) {
    let status = if r.is_healthy() {
        "HEALTHY".green().bold().to_string()
    } else {
        "UNHEALTHY".red().bold().to_string()
    };

    info!("── client {} [{}]", r.inbox_id, status);
    info!(
        "   identity reachable : {}",
        bool_str(r.identity_reachable, true)
    );
    info!("   groups checked     : {}", r.groups.len());

    for g in &r.groups {
        let fork_str = match (g.maybe_forked, g.is_commit_log_forked) {
            (true, _) => format!(
                "{} ({})",
                "FORKED".red().bold(),
                if g.fork_details.is_empty() {
                    "no details".to_string()
                } else {
                    g.fork_details.clone()
                }
            ),
            (_, Some(true)) => "COMMIT LOG FORKED".red().bold().to_string(),
            (false, Some(false)) => "ok".green().bold().to_string(),
            _ => "unknown".to_string(),
        };

        let missing = if g.missing_messages.is_empty() {
            "0".green().bold().to_string()
        } else {
            g.missing_messages.len().to_string().red().bold().to_string()
        };

        info!("   ├─ group {}", g.group_id);
        info!("      missing messages : {missing}");
        info!("      can send         : {}", bool_str(g.can_send, true));
        info!("      can receive      : {}", bool_str(g.can_receive, true));
        info!("      fork status      : {fork_str}");
    }
}
