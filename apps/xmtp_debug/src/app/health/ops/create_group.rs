//! Op: primary creates one new group with default policy and metadata.
//! The new group's id is appended to `ctx.new_groups` so downstream ops
//! and validators see it.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use crate::app::types::InboxId;
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
        let outcome: color_eyre::eyre::Result<(GroupId, InboxId)> = (|| {
            let primary = ctx.primary()?;
            let group = primary.create_group(None, None).map_err(|e| eyre!("{e}"))?;
            let creator = ctx.primary.inbox_id_bytes();
            Ok((group.group_id, creator))
        })();
        match outcome {
            Ok((new_group_id, creator)) => {
                let target = Some(format!("{new_group_id}"));
                ctx.persist_new_group(&new_group_id, creator, vec![creator]);
                ctx.new_groups.push(new_group_id);
                vec![OpResult::from_result(self.name(), target, start, Ok(()))]
            }
            Err(e) => vec![OpResult::fail(self.name(), None, e)],
        }
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
        depends_on: &["CreateIdentity"],
        op: &CreateGroup,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
