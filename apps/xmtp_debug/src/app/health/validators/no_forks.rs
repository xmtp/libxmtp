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
use xmtp_proto::types::GroupId;

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
            let is_transient = ctx.is_transient(&client);
            let db = client.db();
            // Transient is only added to new_groups (see AddMembersToNewGroup);
            // skip existing_groups for transient.
            let check = |gid: &GroupId, out: &mut Vec<OpResult>| {
                let start = Instant::now();
                // Transient may have its group row purged on LeaveGroup
                // depending on libxmtp version — skip with a debug log so
                // unexpected non-leave errors don't slip past silently.
                if is_transient && let Err(e) = client.group(gid) {
                    tracing::debug!(
                        target: "healthcheck",
                        inbox = client.inbox_id(),
                        group = %gid,
                        error = %e,
                        "skip fork check: transient missing group locally",
                    );
                    return;
                }
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
            };
            if !is_transient {
                for gid in &ctx.existing_groups {
                    check(gid, &mut out);
                }
            }
            for gid in &ctx.new_groups {
                check(gid, &mut out);
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
