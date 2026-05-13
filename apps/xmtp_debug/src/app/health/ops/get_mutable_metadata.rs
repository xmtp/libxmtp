//! Op: read mutable_metadata from every group primary is in.
//! Read-only — verifies the metadata extension is reachable.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;

pub struct GetMutableMetadata;

#[async_trait]
impl HealthOp for GetMutableMetadata {
    fn name(&self) -> &'static str {
        "GetMutableMetadata"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = (|| {
                let group = ctx.primary.group(gid).map_err(|e| eyre!("{e}"))?;
                let _ = group.mutable_metadata().map_err(|e| eyre!("{e}"))?;
                Ok(())
            })();
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(gid)),
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
        assert_eq!(GetMutableMetadata.name(), "GetMutableMetadata");
    }
}
