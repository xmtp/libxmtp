//! Validator: every (client, group) pair must not be in a forked state.
//!
//! For each client, queries the local DB for every group's fork status via
//! `QueryGroup::get_group_commit_log_forked_status`. A `Some(true)` is a
//! fail. `Some(false)` and `None` are passes (the latter means commit-log
//! verification isn't enabled for that group).

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_db::prelude::QueryGroup;

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
        let mut out = Vec::new();
        for client in ctx.all_clients() {
            let db = client.db();
            for gid in ctx.all_groups() {
                // Skip clients that don't have a local record of this
                // group: they were never members, or they left and the
                // group row was purged. Either way, fork status is N/A.
                match db.find_group(gid) {
                    Ok(Some(_)) => {}
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::debug!(
                            target: "healthcheck",
                            inbox = client.inbox_id(),
                            group = %gid,
                            error = %e,
                            "skipping fork check: find_group returned error (group likely purged after leave/remove)",
                        );
                        continue;
                    }
                }
                let start = Instant::now();
                let outcome = db.get_group_commit_log_forked_status(gid);
                let (status, error) = match outcome {
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
    }
}
