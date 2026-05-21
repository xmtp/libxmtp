//! Op: rotate + upload a fresh key package for every existing client.
//! Per-client uploads run concurrently — each is an independent network
//! call.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use futures::future::join_all;
use std::time::Instant;

pub struct UploadKeyPackage;

#[async_trait]
impl HealthOp for UploadKeyPackage {
    fn name(&self) -> &'static str {
        "UploadKeyPackage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UploadKeyPackage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let name = self.name();
        let tasks = ctx
            .existing_clients
            .iter()
            .map(|(inbox_id, client)| async move {
                let start = Instant::now();
                let outcome = client.rotate_and_upload_key_package().await;
                let (status, error) = match outcome {
                    Ok(_) => (Status::Pass, None),
                    Err(e) => (Status::Fail, Some(eyre!("{e}"))),
                };
                OpResult {
                    op_name: name,
                    target: Some(hex::encode(inbox_id)),
                    status,
                    duration: start.elapsed(),
                    error,
                }
            });
        join_all(tasks).await
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
