//! Op: rotate + upload a fresh key package for every existing client.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;

pub struct UploadKeyPackage;

#[async_trait]
impl HealthOp for UploadKeyPackage {
    fn name(&self) -> &'static str {
        "UploadKeyPackage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UploadKeyPackage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for (inbox_id, client) in &ctx.existing_clients {
            let start = Instant::now();
            let outcome = client.rotate_and_upload_key_package().await;
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(eyre!("{e}"))),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(inbox_id)),
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
        assert_eq!(UploadKeyPackage.name(), "UploadKeyPackage");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "UploadKeyPackage",
        depends_on: &[],
        make: || Box::new(UploadKeyPackage),
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
