//! Ops that exercise membership changes against groups.
//!
//! - `AddMembersToNewGroup`: primary adds every existing identity to the
//!   newly-created group.
//! - `AddPrimaryToExistingGroups`: for every group in `ctx.existing_groups`,
//!   primary is added by an active member of that group.

use crate::app::health::context::{HealthContext, inbox_id_to_bytes};
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

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "AddMembersToNewGroup")
    )]
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

        let group = match ctx.primary.group(new_group_id.as_slice()) {
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
        let outcome = group.add_members(&inbox_ids).await;
        let (status, error) = match outcome {
            Ok(_) => {
                // Mirror the new membership to redb so subsequent runs
                // see the full member list, not just the creator.
                if let Ok(id_bytes) = <[u8; 16]>::try_from(new_group_id.as_slice()) {
                    let mut members = vec![inbox_id_to_bytes(ctx.primary.inbox_id())];
                    members.extend(inbox_ids.iter().map(|s| inbox_id_to_bytes(s)));
                    ctx.update_group_members(id_bytes, members);
                }
                (Status::Pass, None)
            }
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

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "AddPrimaryToExistingGroups")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let primary_inbox = ctx.primary.inbox_id().to_string();

        for gid in &ctx.existing_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx
                    .existing_clients
                    .values()
                    .filter_map(|c| c.group(gid.as_slice()).ok())
                    .find(|g| g.is_active().unwrap_or(false))
                    .ok_or_else(|| eyre!("no active member found for group"))?;
                group
                    .add_members(std::slice::from_ref(&primary_inbox))
                    .await
                    .map_err(color_eyre::eyre::Report::from)?;
                Ok(())
            }
            .await;

            let (status, error) = match outcome {
                Ok(_) => {
                    if let Ok(id_bytes) = <[u8; 16]>::try_from(gid.as_slice()) {
                        let mut members = ctx.persisted_members(id_bytes);
                        let primary_bytes = inbox_id_to_bytes(&primary_inbox);
                        if !members.contains(&primary_bytes) {
                            members.push(primary_bytes);
                        }
                        ctx.update_group_members(id_bytes, members);
                    }
                    (Status::Pass, None)
                }
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
        depends_on: &["CreateGroup"],
        op: &AddMembersToNewGroup,
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["CreateIdentity"],
        op: &AddPrimaryToExistingGroups,
    }
}
