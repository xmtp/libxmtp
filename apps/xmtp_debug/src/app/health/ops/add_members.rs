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
use futures::FutureExt;
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
        let primary_bytes = ctx.primary.inbox_id_bytes();
        let mut out = ctx
            .for_new_group(self.name(), |ctx, primary, gid| {
                async move {
                    let group = primary.group(&gid)?;
                    let inbox_ids: Vec<String> = ctx
                        .existing_clients
                        .iter()
                        .map(|hc| hc.inbox_id_hex())
                        .chain(ctx.other_identities.iter().map(|hc| hc.inbox_id_hex()))
                        .collect();
                    group.add_members(&inbox_ids).await?;
                    // Mirror the new membership to redb so subsequent runs see
                    // the full member list, not just the creator.
                    let mut members = vec![primary_bytes];
                    members.extend(inbox_ids.iter().map(|s| inbox_id_to_bytes(s)));
                    ctx.update_group_members(&gid, members);
                    Ok(())
                }
                .boxed()
            })
            .await;
        // Welcomes aren't auto-pulled mid-run — sync so newly-added clients
        // can `client.group(...)` immediately in downstream ops.
        if out.iter().any(|r| r.status == Status::Pass)
            && let Err(e) = ctx.sync_welcomes_fanout("AddMembersToNewGroup").await
        {
            tracing::warn!(error = %e, "sync_welcomes_fanout failed");
            // Surface fanout failure as a separate result row so the
            // sweep stays honest about downstream observability.
            out.push(OpResult::fail(self.name(), Some("sync_welcomes".into()), e));
        }
        out
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
        let primary_inbox = ctx.primary.inbox_id_hex();
        let primary_bytes = ctx.primary.inbox_id_bytes();
        let out = ctx
            .for_each_existing_group(self.name(), |ctx, gid| {
                let primary_inbox = primary_inbox.clone();
                async move {
                    let group = ctx
                        .pick_super_admin(&gid)?
                        .ok_or_else(|| eyre!("no active member found for group"))?;
                    group
                        .add_members(std::slice::from_ref(&primary_inbox))
                        .await?;
                    // Adder is a super-admin per pick_super_admin; propagate so
                    // super-admin-only downstream ops don't false-pass.
                    group
                        .update_admin_list(AddSuper, primary_inbox.clone())
                        .await?;
                    let mut members = ctx.persisted_members(&gid);
                    if !members.contains(&primary_bytes) {
                        members.push(primary_bytes);
                    }
                    ctx.update_group_members(&gid, members);
                    Ok(())
                }
                .boxed()
            })
            .await;
        // Welcomes aren't auto-pulled mid-run — sync primary + observers so
        // downstream metadata reads on these groups succeed.
        let mut out = out;
        if out.iter().any(|r| r.status == Status::Pass)
            && let Err(e) = ctx.sync_welcomes_fanout("AddPrimaryToExistingGroups").await
        {
            tracing::warn!(error = %e, "sync_welcomes_fanout failed");
            out.push(OpResult::fail(self.name(), Some("sync_welcomes".into()), e));
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
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["CreateIdentity"],
        op: &AddPrimaryToExistingGroups,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
