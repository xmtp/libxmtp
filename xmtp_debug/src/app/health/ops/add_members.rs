//! Ops that exercise membership changes against groups.
//!
//! - `AddMembersToNewGroup`: primary adds every existing identity to the
//!   newly-created group.
//! - `AddPrimaryToExistingGroups`: for every group in `ctx.existing_groups`,
//!   primary is added by an active member of that group.

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

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "AddMembersToNewGroup"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(new_group_id) = ctx.new_groups.first().cloned() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group; CreateGroup must run first")),
            }];
        };

        let group = match ctx.primary.group(&new_group_id.to_vec()) {
            Ok(g) => g,
            Err(e) => {
                return vec![OpResult {
                    op_name: self.name(),
                    target: Some(format!("{new_group_id}")),
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
        let outcome = group.add_members_by_inbox_id(&inbox_ids).await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(eyre!("{e}"))),
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(format!("{new_group_id}")),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

pub struct AddPrimaryToExistingGroups;

#[async_trait]
impl HealthOp for AddPrimaryToExistingGroups {
    fn name(&self) -> &'static str {
        "AddPrimaryToExistingGroups"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "AddPrimaryToExistingGroups"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let primary_inbox = ctx.primary.inbox_id().to_string();

        for gid in &ctx.existing_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                // Find a client that is an active member of this group.
                let mut adder = None;
                for client in ctx.existing_clients.values() {
                    let Ok(g) = client.group(&gid.to_vec()) else {
                        continue;
                    };
                    if g.is_active().map_err(|e| eyre!("{e}"))? {
                        adder = Some(g);
                        break;
                    }
                }
                let group = adder.ok_or_else(|| eyre!("no active member found for group"))?;
                group
                    .add_members_by_inbox_id(&[primary_inbox.clone()])
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
        assert_eq!(AddMembersToNewGroup.name(), "AddMembersToNewGroup");
        assert_eq!(
            AddPrimaryToExistingGroups.name(),
            "AddPrimaryToExistingGroups"
        );
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "AddMembersToNewGroup",
        depends_on: &["CreateGroup"],
        make: || Box::new(AddMembersToNewGroup),
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "AddPrimaryToExistingGroups",
        depends_on: &["CreateIdentity"],
        make: || Box::new(AddPrimaryToExistingGroups),
    }
}
