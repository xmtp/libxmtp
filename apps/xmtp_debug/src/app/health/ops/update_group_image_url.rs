//! Op: update the group image URL (square) on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

pub struct UpdateGroupImageUrlSquare;

#[async_trait]
impl HealthOp for UpdateGroupImageUrlSquare {
    fn name(&self) -> &'static str {
        "UpdateGroupImageUrlSquare"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdateGroupImageUrlSquare")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_group_image_url_square("https://example.invalid/img.png".into())
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
        assert_eq!(
            UpdateGroupImageUrlSquare.name(),
            "UpdateGroupImageUrlSquare"
        );
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateGroupImageUrlSquare,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
