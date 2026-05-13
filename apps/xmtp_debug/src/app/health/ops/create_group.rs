//! Op: primary creates one new group with default policy and metadata.
//! The new group's id is appended to `ctx.new_groups` so downstream ops
//! and validators see it.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_proto::types::GroupId;

pub struct CreateGroup;

#[async_trait]
impl HealthOp for CreateGroup {
    fn name(&self) -> &'static str {
        "CreateGroup"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "CreateGroup"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let start = Instant::now();
        let outcome = ctx.primary.create_group(None, None);
        let (status, target, error) = match outcome {
            Ok(group) => {
                let new_group_id = GroupId::from(group.group_id.as_slice());
                let hex_id = format!("{new_group_id}");
                ctx.new_groups.push(new_group_id);
                (Status::Pass, Some(hex_id), None)
            }
            Err(e) => (Status::Fail, None, Some(eyre!("{e}"))),
        };
        vec![OpResult {
            op_name: self.name(),
            target,
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(CreateGroup.name(), "CreateGroup");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "CreateGroup",
        depends_on: &["CreateIdentity"],
        make: || Box::new(CreateGroup),
    }
}
