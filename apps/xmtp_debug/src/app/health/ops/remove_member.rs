//! Op: primary admin-removes a freshly-created throwaway identity from
//! the newly-created group via `MlsGroup::remove_members`. The transient
//! victim is created inside this op so nothing persisted gets disturbed
//! (avoids the cross-run "removed-and-stays-around" fork-noise pattern).
//!
//! Distinct from `LeaveGroup` which exercises the MLS self-remove path.
//! Both produce a remove commit but follow different intent + validation
//! flows in libxmtp.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};

pub struct RemoveMember;

#[async_trait]
impl HealthOp for RemoveMember {
    fn name(&self) -> &'static str {
        "RemoveMember"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "RemoveMember"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(gid) = ctx.new_groups.first().cloned() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group to remove from")),
            }];
        };

        let start = Instant::now();
        let mut victim_label = String::from("(uncreated)");
        let outcome: color_eyre::eyre::Result<()> = async {
            let victim = ctx.create_transient().await?;
            let victim_inbox = victim.inbox_id().to_string();
            victim_label = victim_inbox.clone();

            // Primary adds the victim, then admin-removes them via
            // `remove_members`. Both commits go through primary.
            let primary_group = ctx.primary.group(&gid.to_vec()).map_err(|e| eyre!("{e}"))?;
            primary_group
                .add_members(std::slice::from_ref(&victim_inbox))
                .await
                .map_err(|e| eyre!("{e}"))?;
            primary_group
                .remove_members(&[victim_inbox.as_str()])
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
            target: Some(format!("group={gid} victim={victim_label}")),
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
        assert_eq!(RemoveMember.name(), "RemoveMember");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "RemoveMember",
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
        make: || Box::new(RemoveMember),
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
