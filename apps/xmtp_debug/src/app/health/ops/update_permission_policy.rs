//! Op: update the add-member permission policy on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
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
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_permission_policy(
                    PermissionUpdateType::AddMember,
                    PermissionPolicyOption::Allow,
                    None,
                )
                .await?;
            Ok(())
        })
        .await
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
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
