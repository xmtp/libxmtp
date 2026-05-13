//! Validator: every client must have every message every other client has,
//! filtered by the join-time floor (messages sent before a client joined a
//! group are not expected to be present on that client).

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::collections::HashSet;
use std::time::Instant;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::QueryGroupMessage;
use xmtp_proto::types::GroupId;

/// Cached per-(client, group) state: sequence_ids the client has + the
/// earliest application-message timestamp it observed in this group.
struct ClientGroupState {
    inbox: String,
    seqs: HashSet<i64>,
    earliest: Option<i64>,
}

pub struct NoMissingMessages;

#[async_trait]
impl Validator for NoMissingMessages {
    fn name(&self) -> &'static str {
        "NoMissingMessages"
    }

    #[tracing::instrument(target = "healthcheck.validator", skip_all, fields(op = "NoMissingMessages"))]
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();

        let mut all_groups: Vec<GroupId> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().cloned());

        let clients = ctx.all_clients();

        for group_id in &all_groups {
            // Gather each client's local view of this group's messages.
            // We compare in-memory rather than via a SQL "not in (...)"
            // query, since the union of sequence_ids is already in scope.
            let mut per_client: Vec<ClientGroupState> = Vec::new();
            for client in &clients {
                let db = client.db();
                let msgs = match db.get_group_messages(group_id, &MsgQueryArgs::default()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mut seqs = HashSet::new();
                let mut earliest: Option<i64> = None;
                for m in msgs {
                    seqs.insert(m.sequence_id);
                    earliest = Some(match earliest {
                        None => m.sent_at_ns,
                        Some(e) => e.min(m.sent_at_ns),
                    });
                }
                per_client.push(ClientGroupState {
                    inbox: client.inbox_id().to_string(),
                    seqs,
                    earliest,
                });
            }

            // For each client C, compute union of every other client's
            // sequence_ids. Anything in that union but not in C's own set
            // is a missing message — but only if it was sent *after* C's
            // earliest observed message (proxy for "C had joined by then").
            //
            // We re-query the per-other-client message rows for the missing
            // seq ids so we can recover their `sent_at_ns` for the join-time
            // filter. The query is cheap because the message list is local.
            for (i, state) in per_client.iter().enumerate() {
                let join_floor = state.earliest.unwrap_or(i64::MIN);

                let mut missing_count = 0usize;
                for (j, other) in per_client.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let Some(other_client) =
                        clients.iter().find(|c| c.inbox_id() == other.inbox.as_str())
                    else {
                        continue;
                    };
                    let other_msgs = match other_client
                        .db()
                        .get_group_messages(group_id, &MsgQueryArgs::default())
                    {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    for m in other_msgs {
                        if !state.seqs.contains(&m.sequence_id) && m.sent_at_ns >= join_floor {
                            missing_count += 1;
                        }
                    }
                }

                let start = Instant::now();
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
        name: "NoMissingMessages",
        depends_on: &[],
        make: || Box::new(NoMissingMessages),
    }
}
