//! Op: every client sends one message into every group it belongs to.
//! Feeds the missing-messages validator.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;

pub struct SendMessage;

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
                let start = Instant::now();
                let outcome: color_eyre::eyre::Result<()> = async {
                    let Ok(group) = client.group(gid.as_slice()) else {
                        return Ok(());
                    };
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
