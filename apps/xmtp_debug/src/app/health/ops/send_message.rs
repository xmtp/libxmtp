//! Op: every client sends one message into every group it belongs to.
//! Feeds the missing-messages validator.

use crate::DbgClient;
use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_proto::types::GroupId;

pub struct SendMessage;

async fn send_one(op_name: &'static str, client: &Arc<DbgClient>, gid: &GroupId) -> OpResult {
    let start = Instant::now();
    let outcome: color_eyre::eyre::Result<()> = async {
        let group = client.group(gid).map_err(color_eyre::eyre::Report::from)?;
        if !group.is_active().map_err(color_eyre::eyre::Report::from)? {
            return Ok(());
        }
        let body = format!("healthcheck from {}", client.inbox_id());
        group
            .send_message(body.as_bytes(), SendMessageOpts::default())
            .await
            .map_err(color_eyre::eyre::Report::from)?;
        Ok(())
    }
    .await;
    let (status, error) = match outcome {
        Ok(_) => (Status::Pass, None),
        Err(e) => (Status::Fail, Some(e)),
    };
    OpResult {
        op_name,
        target: Some(format!("inbox={} group={gid}", client.inbox_id())),
        status,
        duration: start.elapsed(),
        error,
    }
}

#[async_trait]
impl HealthOp for SendMessage {
    fn name(&self) -> &'static str {
        "SendMessage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "SendMessage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for client in ctx.all_clients() {
            // Transient is only added to new_groups, so existing_groups skip
            // is by design — see AddMembersToNewGroup.
            if !ctx.is_transient(&client) {
                for gid in &ctx.existing_groups {
                    out.push(send_one(self.name(), &client, gid).await);
                }
            }
            for gid in &ctx.new_groups {
                out.push(send_one(self.name(), &client, gid).await);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(SendMessage.name(), "SendMessage");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &SendMessage,
    }
}
