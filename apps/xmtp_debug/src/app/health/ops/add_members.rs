//! Ops that exercise membership changes against groups.
//!
//! - `AddMembersToNewGroup`: primary adds every existing identity to the
//!   newly-created group (the one pushed into `ctx.new_groups` by
//!   `CreateGroup`).
//! - `AddPrimaryToExistingGroups`: filled in by Task 9.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};

pub struct AddMembersToNewGroup;

#[async_trait]
impl HealthOp for AddMembersToNewGroup {
    fn name(&self) -> &'static str {
        "AddMembersToNewGroup"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(&new_group_id) = ctx.new_groups.first() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group; CreateGroup must run first")),
            }];
        };

        let group = match ctx.primary.group(&new_group_id) {
            Ok(g) => g,
            Err(e) => {
                return vec![OpResult {
                    op_name: self.name(),
                    target: Some(hex::encode(new_group_id)),
                    status: Status::Fail,
                    duration: Duration::ZERO,
                    error: Some(eyre!("{e}")),
                }];
            }
        };

        let mut inbox_ids: Vec<String> = ctx
            .existing_clients
            .values()
            .map(|c| c.inbox_id().to_string())
            .collect();
        // Include transient so `LeaveGroup` has a member to remove without
        // disturbing the run-stable primary.
        inbox_ids.push(ctx.transient_identity.inbox_id().to_string());

        let start = Instant::now();
        let outcome = group.add_members(&inbox_ids).await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(eyre!("{e}"))),
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(hex::encode(new_group_id)),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

/// Stub for Task 9. Currently emits no results.
pub struct AddPrimaryToExistingGroups;

#[async_trait]
impl HealthOp for AddPrimaryToExistingGroups {
    fn name(&self) -> &'static str {
        "AddPrimaryToExistingGroups"
    }

    async fn execute(&self, _ctx: &mut HealthContext) -> Vec<OpResult> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable() {
        assert_eq!(AddMembersToNewGroup.name(), "AddMembersToNewGroup");
        assert_eq!(AddPrimaryToExistingGroups.name(), "AddPrimaryToExistingGroups");
    }
}
