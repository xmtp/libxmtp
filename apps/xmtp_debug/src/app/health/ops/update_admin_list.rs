//! Op: add the primary client as a super-admin on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use xmtp_mls::groups::UpdateAdminListType;

/// Make Primary an Admin of all groups
pub struct UpdateAdminList;

#[async_trait]
impl HealthOp for UpdateAdminList {
    fn name(&self) -> &'static str {
        "UpdateAdminList"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateAdminList"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            let inbox = primary.inbox_id().to_string();
            primary
                .group(&gid)?
                .update_admin_list(UpdateAdminListType::AddSuper, inbox)
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
        assert_eq!(UpdateAdminList.name(), "UpdateAdminList");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateAdminList,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
