//! Op: every client sends one message into every group it belongs to.
//! Feeds the missing-messages validator.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_proto::types::GroupId;

pub struct SendMessage;

#[async_trait]
impl HealthOp for SendMessage {
    fn name(&self) -> &'static str {
        "SendMessage"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "SendMessage"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();

        let mut all_groups: Vec<GroupId> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().cloned());

        for client in ctx.all_clients() {
            for gid in &all_groups {
                let start = Instant::now();
                let outcome: color_eyre::eyre::Result<()> = async {
                    let Ok(group) = client.group(&gid.to_vec()) else {
                        return Ok(());
                    };
                    if !group.is_active().map_err(|e| eyre!("{e}"))? {
                        return Ok(());
                    }
                    let body = format!("healthcheck from {}", client.inbox_id());
                    group
                        .send_message(body.as_bytes(), SendMessageOpts::default())
                        .await
                        .map_err(|e| eyre!("{e}"))?;
                    Ok(())
                }
                .await;

                let (status, error) = match outcome {
                    Ok(_) => (Status::Pass, None),
                    Err(e) => (Status::Fail, Some(e)),
                };
                out.push(OpResult {
                    op_name: self.name(),
                    target: Some(format!("inbox={} group={gid}", client.inbox_id())),
                    status,
                    duration: start.elapsed(),
                    error,
                });
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
        op_name: "SendMessage",
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        make: || Box::new(SendMessage),
    }
}
