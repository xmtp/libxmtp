//! Op: rotate the commit-log signer on every group primary is in.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
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
        ctx.for_each_group(self.name(), |primary, gid| async move {
            let signer: Secret = vec![0u8; 32].into();
            primary
                .group(&gid)?
                .update_commit_log_signer(signer)
                .await?;
            Ok(())
        })
        .await
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
