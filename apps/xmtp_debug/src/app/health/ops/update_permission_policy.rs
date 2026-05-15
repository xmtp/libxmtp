//! Op: update the add-member permission policy on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
use xmtp_mls::groups::intents::{PermissionPolicyOption, PermissionUpdateType};

pub struct UpdatePermissionPolicy;

#[async_trait]
impl HealthOp for UpdatePermissionPolicy {
    fn name(&self) -> &'static str {
        "UpdatePermissionPolicy"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdatePermissionPolicy")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(gid)?;
                group
                    .update_permission_policy(
                        PermissionUpdateType::AddMember,
                        PermissionPolicyOption::Allow,
                        None,
                    )
                    .await?;
                Ok(())
            }
            .await;
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(format!("{gid}")),
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
        assert_eq!(UpdatePermissionPolicy.name(), "UpdatePermissionPolicy");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdatePermissionPolicy,
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
