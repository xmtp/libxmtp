//! Op: every client sends one message into every group it belongs to.
//! Feeds the missing-messages validator.
//!
//! On success the message id is written to redb's `MessageStore` so the
//! `NoMissingMessages` validator has an authoritative cross-version set.

use crate::DbgClient;
use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::sync::Arc;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_proto::types::GroupId;

pub struct SendMessage;

async fn send_one(
    op_name: &'static str,
    ctx: &HealthContext,
    client: &Arc<DbgClient>,
    gid: &GroupId,
) -> OpResult {
    let start = Instant::now();
    let outcome: color_eyre::eyre::Result<Option<[u8; 32]>> = async {
        let group = client.group(gid)?;
        if !group.is_active()? {
            return Ok(None);
        }
        let body = format!("healthcheck from {}", client.inbox_id());
        let message_id = group
            .send_message(body.as_bytes(), SendMessageOpts::default())
            .await?;
        let id: [u8; 32] = message_id
            .as_slice()
            .try_into()
            .map_err(|_| eyre!("libxmtp returned non-32-byte message_id"))?;
        Ok(Some(id))
    }
    .await;
    let (status, error) = match outcome {
        Ok(Some(id)) => {
            ctx.record_message(gid, id, client);
            (Status::Pass, None)
        }
        Ok(None) => (Status::Pass, None),
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
            for gid in ctx.all_groups() {
                out.push(send_one(self.name(), ctx, &client, gid).await);
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
        requires: crate::app::health::conditions::Conditions::ALWAYS,
    }
}
