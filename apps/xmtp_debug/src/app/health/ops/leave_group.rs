//! Op: a freshly-created throwaway identity self-removes from the
//! newly-created group via MLS `leave_group`. The transient is created
//! by this op so nothing else in the run is disturbed by the membership
//! change.
//!
//! Distinct from `RemoveMember` which exercises the admin-remove path
//! (one identity removes another via `remove_members`).

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;

pub struct LeaveGroup;

#[async_trait]
impl HealthOp for LeaveGroup {
    fn name(&self) -> &'static str {
        "LeaveGroup"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "LeaveGroup"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(gid) = ctx.new_groups.first().cloned() else {
            return vec![OpResult::fail(
                self.name(),
                None,
                eyre!("no new group to leave"),
            )];
        };

        let start = Instant::now();
        let outcome: color_eyre::eyre::Result<()> = async {
            // Fresh single-use identity. Created locally so it doesn't
            // appear in `ctx.all_clients()` and isn't held to validator
            // convergence checks after the leave.
            let transient = ctx.create_transient().await?;

            // Primary adds the transient to the group, then transient
            // syncs the welcome and leaves.
            let primary = ctx.primary()?;
            let primary_group = primary.group(&gid)?;
            primary_group
                .add_members(&[transient.inbox_id().to_string()])
                .await?;
            transient.sync_welcomes().await?;
            let transient_group = transient.group(&gid)?;
            transient_group.leave_group().await?;
            Ok(())
        }
        .await;
        vec![OpResult::from_result(
            self.name(),
            Some(format!("{gid}")),
            start,
            outcome,
        )]
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
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
