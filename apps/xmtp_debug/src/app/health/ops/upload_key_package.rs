//! Op: rotate + upload a fresh key package for every existing client.
//! Per-client uploads run concurrently — each is an independent network
//! call.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use color_eyre::eyre::eyre;

pub struct UploadKeyPackage;

#[async_trait]
impl HealthOp for UploadKeyPackage {
    fn name(&self) -> &'static str {
        "UploadKeyPackage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UploadKeyPackage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_client(self.name(), |client| async move {
            client
                .rotate_and_upload_key_package()
                .await
                .map_err(|e| eyre!("{e}"))?;
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
        assert_eq!(UploadKeyPackage.name(), "UploadKeyPackage");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &[],
        op: &UploadKeyPackage,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
