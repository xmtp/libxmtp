//! Op: add the primary client as a super-admin on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
use xmtp_mls::groups::UpdateAdminListType;

/// Make Primary an Admin of all groups
pub struct UpdateAdminList;

#[async_trait]
impl HealthOp for UpdateAdminList {
    fn name(&self) -> &'static str {
        "UpdateAdminList"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateAdminList"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(gid)?;
                group
                    .update_admin_list(
                        UpdateAdminListType::AddSuper,
                        ctx.primary.inbox_id().to_string(),
                    )
                    .await?;
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
    fn name_is_stable() {
        assert_eq!(UpdateAdminList.name(), "UpdateAdminList");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateAdminList,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
