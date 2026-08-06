//! Op: read mutable_metadata from every group primary is in.
//! Read-only — verifies the metadata extension is reachable.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

pub struct GetMutableMetadata;

#[async_trait]
impl HealthOp for GetMutableMetadata {
    fn name(&self) -> &'static str {
        "GetMutableMetadata"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "GetMutableMetadata"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary.group(&gid)?.mutable_metadata()?;
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
        assert_eq!(GetMutableMetadata.name(), "GetMutableMetadata");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &GetMutableMetadata,
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
