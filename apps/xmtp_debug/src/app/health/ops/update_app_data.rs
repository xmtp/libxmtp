//! Op: update the app-data metadata field on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

pub struct UpdateAppData;

#[async_trait]
impl HealthOp for UpdateAppData {
    fn name(&self) -> &'static str {
        "UpdateAppData"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateAppData"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_app_data("healthcheck-app-data".into())
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
        assert_eq!(UpdateAppData.name(), "UpdateAppData");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateAppData,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
