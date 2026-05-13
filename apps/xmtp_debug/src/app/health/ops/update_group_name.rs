//! Op: update the group name on every reachable group.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;

pub struct UpdateGroupName;

#[async_trait]
impl HealthOp for UpdateGroupName {
    fn name(&self) -> &'static str {
        "UpdateGroupName"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateGroupName"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx
                    .primary
                    .group(gid.as_slice())
                    .map_err(color_eyre::eyre::Report::from)?;
                group
                    .update_group_name("healthcheck-name".into())
                    .await
                    .map_err(color_eyre::eyre::Report::from)?;
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
        assert_eq!(UpdateGroupName.name(), "UpdateGroupName");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateGroupName,
    }
}
