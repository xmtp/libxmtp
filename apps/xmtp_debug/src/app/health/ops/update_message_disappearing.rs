//! Op: set and remove conversation message-disappearing settings on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;

pub struct UpdateMessageDisappearing;

#[async_trait]
impl HealthOp for UpdateMessageDisappearing {
    fn name(&self) -> &'static str {
        "UpdateMessageDisappearing"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdateMessageDisappearing")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_conversation_message_disappearing_settings(
                    MessageDisappearingSettings::new(1, 86_400_000_000_000),
                )
                .await?;
            Ok(())
        })
        .await
    }
}

pub struct RemoveMessageDisappearing;

#[async_trait]
impl HealthOp for RemoveMessageDisappearing {
    fn name(&self) -> &'static str {
        "RemoveMessageDisappearing"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "RemoveMessageDisappearing")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .remove_conversation_message_disappearing_settings()
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
    fn names_are_stable() {
        assert_eq!(
            UpdateMessageDisappearing.name(),
            "UpdateMessageDisappearing"
        );
        assert_eq!(
            RemoveMessageDisappearing.name(),
            "RemoveMessageDisappearing"
        );
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateMessageDisappearing,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["UpdateMessageDisappearing"],
        op: &RemoveMessageDisappearing,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
