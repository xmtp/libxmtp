//! Op: update the group description on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

pub struct UpdateGroupDescription;

#[async_trait]
impl HealthOp for UpdateGroupDescription {
    fn name(&self) -> &'static str {
        "UpdateGroupDescription"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdateGroupDescription")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_group_description("healthcheck-desc".into())
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
        assert_eq!(UpdateGroupDescription.name(), "UpdateGroupDescription");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateGroupDescription,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
