//! Validator: every (client, group) pair must not be in a forked state.
//!
//! Per client: force a fresh reconciliation tick so the DB column is current
//! at check time, then read `is_commit_log_forked`. Without this, the
//! healthcheck races the periodic CommitLogWorker and reports stale state.

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_db::prelude::QueryGroup;
use xmtp_mls::groups::commit_log::CommitLogWorker;

pub struct NoForkedGroups;

#[async_trait]
impl Validator for NoForkedGroups {
    fn name(&self) -> &'static str {
        "NoForkedGroups"
    }

    #[tracing::instrument(
        target = "healthcheck.validator",
        skip_all,
        fields(op = "NoForkedGroups")
    )]
    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let clients = match ctx.all_clients() {
            Ok(cs) => cs,
            Err(e) => {
                return vec![OpResult {
                    op_name: self.name(),
                    target: None,
                    status: Status::Fail,
                    duration: std::time::Duration::ZERO,
                    error: Some(e),
                }];
            }
        };
        let mut out = Vec::new();
        for client in &clients {
            let db = client.db();

            // Force a reconciliation pass so fork status reflects what's on
            // the server *now*, not what the last periodic tick wrote.
            let tick_start = Instant::now();
            let mut worker = CommitLogWorker::new(client.context.clone());
            if let Err(e) = worker.tick().await {
                out.push(OpResult {
                    op_name: self.name(),
                    target: Some(format!("inbox={} reconcile", client.inbox_id())),
                    status: Status::Fail,
                    duration: tick_start.elapsed(),
                    error: Some(eyre!("commit-log reconcile failed: {e}")),
                });
                continue;
            }

            for gid in ctx.all_groups() {
                // Skip clients that aren't active members. A removed
                // client's frozen local commit-log diverges from the
                // live one by design; that's not a fork worth flagging.
                let is_active = client
                    .group(gid)
                    .ok()
                    .and_then(|g| g.is_active().ok())
                    .unwrap_or(false);
                if !is_active {
                    continue;
                }
                let start = Instant::now();
                let (status, error) = match db.get_group_commit_log_forked_status(gid) {
                    Ok(Some(true)) => (Status::Fail, Some(eyre!("group forked"))),
                    Ok(_) => (Status::Pass, None),
                    Err(e) => (Status::Fail, Some(eyre!("{e}"))),
                };
                out.push(OpResult {
                    op_name: self.name(),
                    target: Some(format!("inbox={} group={gid}", client.inbox_id())),
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
        assert_eq!(NoForkedGroups.name(), "NoForkedGroups");
    }
}

inventory::submit! {
    crate::app::health::validators::ValidatorEntry {
        depends_on: &[],
        validator: &NoForkedGroups,
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
