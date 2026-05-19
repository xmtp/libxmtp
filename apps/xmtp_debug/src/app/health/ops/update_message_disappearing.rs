//! Op: set and remove conversation message-disappearing settings on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
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
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx
                    .primary
                    .group(gid)
                    .map_err(color_eyre::eyre::Report::from)?;
                group
                    .update_conversation_message_disappearing_settings(
                        MessageDisappearingSettings::new(1, 86_400_000_000_000),
                    )
                    .await
                    .map_err(color_eyre::eyre::Report::from)?;
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
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx
                    .primary
                    .group(gid)
                    .map_err(color_eyre::eyre::Report::from)?;
                group
                    .remove_conversation_message_disappearing_settings()
                    .await
                    .map_err(color_eyre::eyre::Report::from)?;
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
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["UpdateMessageDisappearing"],
        op: &RemoveMessageDisappearing,
    }
}
