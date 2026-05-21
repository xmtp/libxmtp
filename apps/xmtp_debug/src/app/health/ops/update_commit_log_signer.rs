//! Op: rotate the commit-log signer on every group primary is in.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
use xmtp_cryptography::Secret;

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
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = async {
                let group = ctx.primary.group(gid)?;
                let signer: Secret = vec![0u8; 32].into();
                group.update_commit_log_signer(signer).await?;
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
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateCommitLogSigner,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
