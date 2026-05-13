//! Op: rotate the commit-log signer on every group primary is in.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_cryptography::Secret;
use xmtp_proto::types::GroupId;

pub struct UpdateCommitLogSigner;

#[async_trait]
impl HealthOp for UpdateCommitLogSigner {
    fn name(&self) -> &'static str {
        "UpdateCommitLogSigner"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdateCommitLogSigner")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<GroupId> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().cloned());

        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(&gid.to_vec()).map_err(|e| eyre!("{e}"))?;
                let signer: Secret = vec![0u8; 32].into();
                group
                    .update_commit_log_signer(signer)
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
    fn name_is_stable() {
        assert_eq!(UpdateCommitLogSigner.name(), "UpdateCommitLogSigner");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "UpdateCommitLogSigner",
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        make: || Box::new(UpdateCommitLogSigner),
    }
}
