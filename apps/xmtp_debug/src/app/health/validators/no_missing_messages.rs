//! Validator: every client must have every message every other client has,
//! filtered by the join-time floor (messages sent before a client joined a
//! group are not expected to be present on that client).

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use xmtp_db::group::QueryGroup;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::QueryGroupMessage;

/// Per-(client, group) state cached while inspecting a single group.
struct ClientGroupState {
    inbox: String,
    /// Sequence_ids this client has stored locally for this group.
    seqs: HashSet<i64>,
    /// Authoritative join time: `StoredGroup::created_at_ns` (the welcome
    /// timestamp for invited members, group creation for the creator).
    join_at_ns: i64,
}

pub struct NoMissingMessages;

#[async_trait]
impl Validator for NoMissingMessages {
    fn name(&self) -> &'static str {
        "NoMissingMessages"
    }

    #[tracing::instrument(
        target = "healthcheck.validator",
        skip_all,
        fields(op = "NoMissingMessages")
    )]
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let clients = ctx.all_clients();

        for group_id in ctx.all_groups() {
            // Members of this group only — skip clients that aren't part of it.
            let mut per_client: Vec<ClientGroupState> = Vec::new();
            // Authoritative seq → sent_at_ns map across every member's
            // local store. Built once, consulted by every per-client diff.
            let mut sent_at: HashMap<i64, i64> = HashMap::new();

            for client in &clients {
                let db = client.db();
                let stored = match db.find_group(group_id) {
                    Ok(Some(g)) => g,
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::warn!(
                            target: "healthcheck",
                            inbox = client.inbox_id(),
                            group = %group_id,
                            error = %e,
                            "skipping client/group: find_group failed",
                        );
                        continue;
                    }
                };
                // Skip clients that left or were removed: their local view
                // is intentionally frozen at the moment of removal and will
                // not see post-removal commits. The MLS group's
                // `is_active()` returns false for those clients.
                let active = client
                    .group(group_id.as_slice())
                    .ok()
                    .and_then(|g| g.is_active().ok())
                    .unwrap_or(false);
                if !active {
                    tracing::debug!(
                        target: "healthcheck",
                        inbox = client.inbox_id(),
                        group = %group_id,
                        "skipping inactive client (left or removed)",
                    );
                    continue;
                }
                let msgs = match db.get_group_messages(group_id, &MsgQueryArgs::default()) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(
                            target: "healthcheck",
                            inbox = client.inbox_id(),
                            group = %group_id,
                            error = %e,
                            "skipping client/group: get_group_messages failed",
                        );
                        continue;
                    }
                };
                let mut seqs = HashSet::new();
                for m in msgs {
                    seqs.insert(m.sequence_id);
                    // First writer wins — sent_at_ns is invariant per seq.
                    sent_at.entry(m.sequence_id).or_insert(m.sent_at_ns);
                }
                per_client.push(ClientGroupState {
                    inbox: client.inbox_id().to_string(),
                    seqs,
                    join_at_ns: stored.created_at_ns,
                });
            }

            // Union of every member's seqs. A client is missing seq `s` iff
            // `s ∈ global_union \ client.seqs` AND `sent_at[s] >= client.join_at_ns`.
            let global_union: HashSet<i64> = per_client
                .iter()
                .flat_map(|s| s.seqs.iter().copied())
                .collect();

            for state in &per_client {
                let start = Instant::now();
                let missing_count = global_union
                    .difference(&state.seqs)
                    .filter(|seq| match sent_at.get(seq) {
                        Some(&ts) => ts >= state.join_at_ns,
                        None => false,
                    })
                    .count();

                let (status, error) = if missing_count == 0 {
                    (Status::Pass, None)
                } else {
                    (
                        Status::Fail,
                        Some(eyre!("{missing_count} missing messages")),
                    )
                };
                out.push(OpResult {
                    op_name: self.name(),
                    target: Some(format!("inbox={} group={group_id}", state.inbox)),
                    status,
                    duration: start.elapsed(),
                    error,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(NoMissingMessages.name(), "NoMissingMessages");
    }
}

inventory::submit! {
    crate::app::health::validators::ValidatorEntry {
        depends_on: &[],
        validator: &NoMissingMessages,
    }
}
