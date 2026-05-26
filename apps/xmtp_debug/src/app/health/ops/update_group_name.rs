//! Op: update the group name on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

pub struct UpdateGroupName;

#[async_trait]
impl HealthOp for UpdateGroupName {
    fn name(&self) -> &'static str {
        "UpdateGroupName"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateGroupName"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_group_name("healthcheck-name".into())
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
        assert_eq!(UpdateGroupName.name(), "UpdateGroupName");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateGroupName,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
