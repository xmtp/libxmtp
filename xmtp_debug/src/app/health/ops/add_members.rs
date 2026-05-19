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
use xmtp_mls::groups::UpdateAdminListType::*;

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

        let inbox_ids: Vec<String> = ctx
            .existing_clients
            .values()
            .map(|c| c.inbox_id().to_string())
            .chain(ctx.other_identities.iter().map(hex::encode))
            .collect();

        let start = Instant::now();
        let outcome = group.add_members_by_inbox_id(&inbox_ids).await;
        let (status, error) = match outcome {
            Ok(_) => {
                if let Ok(id_bytes) = <[u8; 16]>::try_from(new_group_id.as_slice()) {
                    let mut members = vec![inbox_id_to_bytes(ctx.primary.inbox_id())];
                    members.extend(inbox_ids.iter().map(|s| inbox_id_to_bytes(s)));
                    ctx.update_group_members(id_bytes, members);
                }
                (Status::Pass, None)
            }
            Err(e) => (Status::Fail, Some(eyre!("{e}"))),
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
                let id_bytes = <[u8; 16]>::try_from(gid.as_slice()).map_err(|_| {
                    eyre!(
                        "expected 16-byte group_id, got {} bytes",
                        gid.as_slice().len()
                    )
                })?;
                // Prefer super-admin adder so AddSuper promote below
                // succeeds; fall back to any active member.
                let candidates: Vec<_> = ctx
                    .existing_clients
                    .values()
                    .filter_map(|c| c.group(&gid.to_vec()).ok().map(|g| (c, g)))
                    .filter(|(_, g)| g.is_active().unwrap_or(false))
                    .collect();
                let group = candidates
                    .iter()
                    .find(|(c, g)| {
                        g.is_super_admin(c.inbox_id().to_string()).unwrap_or(false)
                    })
                    .or_else(|| candidates.first())
                    .map(|(_, g)| g.clone())
                    .ok_or_else(|| eyre!("no active member found for group"))?;
                group
                    .add_members_by_inbox_id(std::slice::from_ref(&primary_inbox))
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                // Adder was picked as super-admin; propagate failure so
                // super-admin-only downstream ops don't false-pass.
                group.update_admin_list(AddSuper, primary_inbox.clone()).await.map_err(|e| eyre!("{e}"))?;
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
        op_name: "AddMembersToNewGroup",
        depends_on: &["CreateGroup"],
        make: || Box::new(AddMembersToNewGroup),
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "AddPrimaryToExistingGroups",
        depends_on: &["CreateIdentity"],
        make: || Box::new(AddPrimaryToExistingGroups),
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
