//! Op: transient identity self-removes from the newly-created group via
//! MLS `leave_group`. Uses the transient instead of the primary so the
//! persisted primary stays in every group across runs.
//! Must run last so prior ops are not invalidated by the membership change.
//!
//! Distinct from `RemoveMember` which exercises the admin-remove path
//! (one identity removes another via `remove_members`).

use crate::app::health::context::{HealthContext, group_id_bytes, inbox_id_to_bytes};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};

pub struct LeaveGroup;

#[async_trait]
impl HealthOp for LeaveGroup {
    fn name(&self) -> &'static str {
        "LeaveGroup"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "LeaveGroup"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(gid) = ctx.new_groups.first().cloned() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group to leave")),
            }];
        };

        let start = Instant::now();
        let outcome: color_eyre::eyre::Result<()> = async {
            // Transient must see the welcome for this group before it can
            // load + leave it. The add happened earlier in the run but is
            // only visible after a sync.
            ctx.transient_identity
                .sync_welcomes()
                .await
                .map_err(color_eyre::eyre::Report::from)?;
            let group = ctx
                .transient_identity
                .group(&gid)
                .map_err(color_eyre::eyre::Report::from)?;
            group
                .leave_group()
                .await
                .map_err(color_eyre::eyre::Report::from)?;
            let id_bytes = group_id_bytes(&gid)?;
            let transient_bytes = inbox_id_to_bytes(ctx.transient_identity.inbox_id());
            let members: Vec<_> = ctx
                .persisted_members(id_bytes)
                .into_iter()
                .filter(|m| m != &transient_bytes)
                .collect();
            ctx.update_group_members(id_bytes, members);
            Ok(())
        }
        .await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(e)),
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(format!("{gid}")),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(LeaveGroup.name(), "LeaveGroup");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &[
            "SendMessage",
            "UpdateGroupName",
            "UpdateGroupDescription",
            "UpdateGroupImageUrlSquare",
            "RemoveMessageDisappearing",
            "UpdateAdminList",
            "UpdatePermissionPolicy",
            "UpdateAppData",
            "UpdateCommitLogSigner",
            "UpdateConsentStateQuiet",
            "GetMutableMetadata",
        ],
        op: &LeaveGroup,
    }
}
