//! Ops that exercise membership changes against groups.
//!
//! - `AddMembersToNewGroup`: primary adds every existing identity to the
//!   newly-created group.
//! - `AddPrimaryToExistingGroups`: for every group in `ctx.existing_groups`,
//!   primary is added by an active member of that group.

use crate::app::health::context::{HealthContext, group_id_bytes, inbox_id_to_bytes};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};
use xmtp_mls::groups::UpdateAdminListType;

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
        let outcome: color_eyre::eyre::Result<()> = async {
            group
                .add_members(&inbox_ids)
                .await
                .map_err(color_eyre::eyre::Report::from)?;
            // Mirror the new membership to redb so subsequent runs see
            // the full member list, not just the creator.
            let id_bytes = group_id_bytes(&new_group_id)?;
            let mut members = vec![inbox_id_to_bytes(ctx.primary.inbox_id())];
            members.extend(inbox_ids.iter().map(|s| inbox_id_to_bytes(s)));
            ctx.update_group_members(id_bytes, members);
            Ok(())
        }
        .await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(e)),
        };
        // Welcomes aren't auto-pulled mid-run — sync so newly-added clients
        // can `client.group(...)` immediately in downstream ops.
        if status == Status::Pass {
            ctx.sync_welcomes_fanout("AddMembersToNewGroup").await;
        }
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
                let id_bytes = group_id_bytes(gid)?;
                // Prefer the persisted creator (super-admin) so we can also
                // promote primary below. Fall back to any active member if
                // the creator isn't available locally — the add will still
                // succeed but the promote will be skipped.
                let creator = ctx.persisted_creator(id_bytes);
                let adder = creator
                    .and_then(|c| ctx.existing_clients.get(&c))
                    .and_then(|c| c.group(gid.as_slice()).ok())
                    .filter(|g| g.is_active().unwrap_or(false));
                let group = adder
                    .or_else(|| {
                        ctx.existing_clients
                            .values()
                            .filter_map(|c| c.group(gid.as_slice()).ok())
                            .find(|g| g.is_active().unwrap_or(false))
                    })
                    .ok_or_else(|| eyre!("no active member found for group"))?;
                group
                    .add_members(std::slice::from_ref(&primary_inbox))
                    .await
                    .map_err(color_eyre::eyre::Report::from)?;
                // Promote primary to super-admin so downstream metadata ops
                // (UpdateAdminList, UpdateMessageDisappearing,
                // UpdatePermissionPolicy) can run on this prior-version
                // group. Best-effort: if the adder isn't super-admin, this
                // fails and we log but don't abort the run.
                if let Err(e) = group
                    .update_admin_list(UpdateAdminListType::AddSuper, primary_inbox.clone())
                    .await
                {
                    tracing::warn!(
                        target: "healthcheck",
                        group = %gid,
                        error = %e,
                        "failed to promote primary to super-admin on existing group; \
                         metadata ops requiring admin will fail on this group",
                    );
                }
                let mut members = ctx.persisted_members(id_bytes);
                let primary_bytes = inbox_id_to_bytes(&primary_inbox);
                if !members.contains(&primary_bytes) {
                    members.push(primary_bytes);
                }
                ctx.update_group_members(id_bytes, members);
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

        // Welcomes aren't auto-pulled mid-run — sync primary + observers so
        // downstream metadata reads on these groups succeed.
        if out.iter().any(|r| r.status == Status::Pass) {
            ctx.sync_welcomes_fanout("AddPrimaryToExistingGroups").await;
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
