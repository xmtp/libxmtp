//! Op: primary removes the transient identity from the newly-created
//! group (the one from `CreateGroup`). Uses the transient instead of the
//! primary so the persisted primary stays in every group across runs.
//! Must run last so prior ops are not invalidated by the membership change.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};

pub struct LeaveGroup;

#[async_trait]
impl HealthOp for LeaveGroup {
    fn name(&self) -> &'static str {
        "LeaveGroup"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(&gid) = ctx.new_groups.first() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group to leave")),
            }];
        };

        let transient_inbox = ctx.transient_identity.inbox_id().to_string();
        let start = Instant::now();
        let outcome: color_eyre::eyre::Result<()> = async {
            let group = ctx.primary.group(&gid).map_err(|e| eyre!("{e}"))?;
            group
                .remove_members(&[transient_inbox.as_str()])
                .await
                .map_err(|e| eyre!("{e}"))?;
            Ok(())
        }
        .await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(e)),
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(hex::encode(gid)),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(LeaveGroup.name(), "LeaveGroup");
    }
}
