//! Op: every client sends one message into every group it belongs to.
//! Feeds the missing-messages validator.
//!
//! On success the message id is written to redb's `MessageStore` so the
//! `NoMissingMessages` validator has an authoritative cross-version set.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;

pub struct SendMessage;

#[async_trait]
impl HealthOp for SendMessage {
    fn name(&self) -> &'static str {
        "SendMessage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "SendMessage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let ctx_ref = &*ctx;
        ctx_ref
            .for_each_client_group(self.name(), |client, gid| async move {
                // Non-membership = client.group(gid) errors. Pass with no
                // send so we distinguish "not in group" from "send broke".
                let Ok(group) = client.group(&gid) else {
                    return Ok(());
                };
                if !group.is_active()? {
                    return Ok(());
                }
                let body = format!("healthcheck from {}", client.inbox_id());
                let message_id = group
                    .send_message(body.as_bytes(), SendMessageOpts::default())
                    .await?;
                let id: [u8; 32] = message_id
                    .as_slice()
                    .try_into()
                    .map_err(|_| eyre!("libxmtp returned non-32-byte message_id"))?;
                ctx_ref.record_message(&gid, id, &client);
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
