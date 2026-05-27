//! Op: a freshly-created throwaway identity self-removes from the
//! newly-created group via MLS `leave_group`. The transient is created
//! by this op so nothing else in the run is disturbed by the membership
//! change.
//!
//! Distinct from `RemoveMember` which exercises the admin-remove path
//! (one identity removes another via `remove_members`).

use crate::app::health::context::HealthContext;
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
            // Fresh single-use identity. Created locally so it doesn't
            // appear in `ctx.all_clients()` and isn't held to validator
            // convergence checks after the leave.
            let transient = ctx.create_transient().await?;

            // Primary adds the transient to the group, then transient
            // syncs the welcome and leaves.
            let primary_group = ctx.primary.group(&gid.to_vec()).map_err(|e| eyre!("{e}"))?;
            primary_group
                .add_members(&[transient.inbox_id().to_string()])
                .await
                .map_err(|e| eyre!("{e}"))?;
            transient.sync_welcomes().await.map_err(|e| eyre!("{e}"))?;
            let transient_group = transient.group(&gid.to_vec()).map_err(|e| eyre!("{e}"))?;
            transient_group
                .leave_group()
                .await
                .map_err(|e| eyre!("{e}"))?;
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
        op_name: "LeaveGroup",
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
        make: || Box::new(LeaveGroup),
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
