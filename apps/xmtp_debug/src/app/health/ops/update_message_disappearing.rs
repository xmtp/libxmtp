//! Op: set and remove conversation message-disappearing settings on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;

pub struct UpdateMessageDisappearing;

#[async_trait]
impl HealthOp for UpdateMessageDisappearing {
    fn name(&self) -> &'static str {
        "UpdateMessageDisappearing"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(gid).map_err(|e| eyre!("{e}"))?;
                group
                    .update_conversation_message_disappearing_settings(
                        MessageDisappearingSettings::new(1, 86_400_000_000_000),
                    )
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            }
            .await;
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(gid)),
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

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(gid).map_err(|e| eyre!("{e}"))?;
                group
                    .remove_conversation_message_disappearing_settings()
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            }
            .await;
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(gid)),
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
        assert_eq!(UpdateMessageDisappearing.name(), "UpdateMessageDisappearing");
        assert_eq!(RemoveMessageDisappearing.name(), "RemoveMessageDisappearing");
    }
}
