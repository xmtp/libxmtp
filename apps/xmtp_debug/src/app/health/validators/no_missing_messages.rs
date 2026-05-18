//! Validator: every active member of a group must have every message that
//! redb's `MessageStore` recorded for that group. Cross-version,
//! authoritative — independent of any single client's local libxmtp DB.

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_db::group::QueryGroup;
use xmtp_db::prelude::QueryGroupMessage;

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
        // Load everything once — `recorded_messages` per-group would
        // rescan the whole network per iteration.
        let by_group = ctx.recorded_messages_by_group();

        // Collect every (client, group, active_mls_group) we'll inspect.
        // Inactive clients (left/removed) aren't held to convergence.
        let mut pairs = Vec::new();
        for group_id in ctx.all_groups() {
            let Some(expected) = by_group.get(group_id) else {
                continue;
            };
            if expected.is_empty() {
                continue;
            }
            for client in &clients {
                let Ok(mls_group) = client.group(group_id) else {
                    continue;
                };
                // Skip clients that left or were removed: their local view
                // is intentionally frozen at the moment of removal and will
                // not see post-removal commits.
                if !mls_group.is_active().unwrap_or(false) {
                    continue;
                }
                pairs.push((client.clone(), group_id, mls_group, expected));
            }
        }

        // Pre-sync every pair in parallel so we don't false-positive on
        // commits that haven't reached the local DB yet (own sends and
        // concurrent peer sends both qualify).
        futures::future::join_all(pairs.iter().map(
            |(client, group_id, mls_group, _)| async move {
                if let Err(e) = mls_group.sync_with_conn().await {
                    tracing::debug!(
                        target: "healthcheck",
                        inbox = client.inbox_id(),
                        group = %group_id,
                        error = %e,
                        "sync_with_conn before NoMissingMessages check failed",
                    );
                }
            },
        ))
        .await;

        for (client, group_id, _mls_group, expected) in &pairs {
            let start = Instant::now();
            let db = client.db();
            // Authoritative join floor: messages older than the group's
            // local `created_at_ns` were sent before this client joined
            // and aren't expected to appear in their store.
            let join_at_ns = match db.find_group(group_id) {
                Ok(Some(g)) => g.created_at_ns,
                Ok(None) => {
                    tracing::debug!(
                        target: "healthcheck",
                        inbox = client.inbox_id(),
                        group = %group_id,
                        "skipping NoMissingMessages: client has no local group row",
                    );
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        target: "healthcheck",
                        inbox = client.inbox_id(),
                        group = %group_id,
                        error = %e,
                        "find_group failed; skipping client/group",
                    );
                    continue;
                }
            };

            let mut expected_after_join = 0usize;
            let mut missing = 0usize;
            for msg in *expected {
                if msg.sent_at_ns < join_at_ns {
                    continue;
                }
                expected_after_join += 1;
                match db.get_group_message(msg.id) {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        missing += 1;
                        tracing::warn!(
                            target: "healthcheck",
                            inbox = client.inbox_id(),
                            group = %group_id,
                            message_id = %hex::encode(msg.id),
                            sender = %hex::encode(msg.sender_inbox_id),
                            sent_at_ns = msg.sent_at_ns,
                            recorded_by_version = %msg.xdbg_version,
                            "missing recorded message in local DB",
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "healthcheck",
                            inbox = client.inbox_id(),
                            group = %group_id,
                            error = %e,
                            message_id = %hex::encode(msg.id),
                            "get_group_message error; counting as missing",
                        );
                        missing += 1;
                    }
                }
            }

            let (status, error) = if missing == 0 {
                (Status::Pass, None)
            } else {
                (
                    Status::Fail,
                    Some(eyre!(
                        "{missing}/{expected_after_join} expected messages missing"
                    )),
                )
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(format!("inbox={} group={group_id}", client.inbox_id())),
                status,
                duration: start.elapsed(),
                error,
            });
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
