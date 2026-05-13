//! Op: primary admin-removes a peer from the newly-created group via
//! `MlsGroup::remove_members`. Distinct from `LeaveGroup` which exercises
//! the MLS self-remove path. Both code paths produce a remove commit but
//! follow different intent + validation flows in libxmtp.
//!
//! Victim is the first existing identity (i.e. one of the bootstrap or
//! pre-existing clients). If none is available, the op fails with a
//! reason.

use crate::app::health::context::{HealthContext, group_id_bytes, inbox_id_to_bytes};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::{Duration, Instant};

pub struct RemoveMember;

#[async_trait]
impl HealthOp for RemoveMember {
    fn name(&self) -> &'static str {
        "RemoveMember"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "RemoveMember"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(gid) = ctx.new_groups.first().cloned() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no new group to remove from")),
            }];
        };

        let primary_inbox = ctx.primary.inbox_id().to_string();
        let transient_inbox = ctx.transient_identity.inbox_id().to_string();
        let Some(victim_inbox) = ctx
            .existing_clients
            .values()
            .map(|c| c.inbox_id().to_string())
            .find(|id| id != &primary_inbox && id != &transient_inbox)
        else {
            return vec![OpResult {
                op_name: self.name(),
                target: Some(format!("{gid}")),
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!("no existing identity available as remove target")),
            }];
        };

        let start = Instant::now();
        let outcome: color_eyre::eyre::Result<()> = async {
            let group = ctx
                .primary
                .group(gid.as_slice())
                .map_err(color_eyre::eyre::Report::from)?;
            group
                .remove_members(&[victim_inbox.as_str()])
                .await
                .map_err(color_eyre::eyre::Report::from)?;
            let id_bytes = group_id_bytes(&gid)?;
            drop_member_from_persisted_group(ctx, id_bytes, &victim_inbox);
            Ok(())
        }
        .await;
        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(e)),
        };
        vec![OpResult {
            op_name: self.name(),
            target: Some(format!("group={gid} victim={victim_inbox}")),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

/// Read the persisted group's members from redb, drop `victim_inbox`,
/// and write back. Panics if redb is unreachable — a redb failure
/// indicates an xdbg state-directory issue, not an op-level failure.
fn drop_member_from_persisted_group(
    ctx: &HealthContext,
    group_id_bytes: [u8; 16],
    victim_inbox: &str,
) {
    let victim_bytes = inbox_id_to_bytes(victim_inbox);
    let members: Vec<_> = ctx
        .persisted_members(group_id_bytes)
        .into_iter()
        .filter(|m| m != &victim_bytes)
        .collect();
    ctx.update_group_members(group_id_bytes, members);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(RemoveMember.name(), "RemoveMember");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &[
            "SendMessage",
            "UpdateGroupName",
            "UpdateGroupDescription",
            "UpdateGroupImageUrlSquare",
            "RemoveMessageDisappearing",
            "UpdateAdminList",
            "UpdatePermissionPolicy",
            "UpdateAppData",
            "UpdateCommitLogSigner",
            "UpdateConsentStateQuiet",
            "GetMutableMetadata",
        ],
        op: &RemoveMember,
    }
}
