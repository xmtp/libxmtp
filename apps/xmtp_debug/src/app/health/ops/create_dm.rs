//! Op: primary creates a DM with every existing peer and round-trips one
//! message in each direction.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;

pub struct CreateDm;

#[async_trait]
impl HealthOp for CreateDm {
    fn name(&self) -> &'static str {
        "CreateDm"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "CreateDm"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let primary_inbox = ctx.primary.inbox_id().to_string();

        for (peer_inbox_bytes, peer) in &ctx.existing_clients {
            let peer_inbox = peer.inbox_id().to_string();
            if peer_inbox == primary_inbox {
                continue;
            }

            // Direction A: primary → peer.
            let start = Instant::now();
            let dir_a: color_eyre::eyre::Result<()> = async {
                let dm = ctx
                    .primary
                    .find_or_create_dm(peer_inbox.as_str(), None)
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                dm.send_message(b"hi from primary", SendMessageOpts::default())
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            }
            .await;
            let (status, error) = match dir_a {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: "CreateDm",
                target: Some(format!("primary->{}", hex::encode(peer_inbox_bytes))),
                status,
                duration: start.elapsed(),
                error,
            });

            // Direction B: peer → primary.
            let start = Instant::now();
            let dir_b: color_eyre::eyre::Result<()> = async {
                peer.sync_all_welcomes_and_groups(None)
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                let dm = peer
                    .find_or_create_dm(primary_inbox.as_str(), None)
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                dm.send_message(b"hi from peer", SendMessageOpts::default())
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            }
            .await;
            let (status, error) = match dir_b {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: "SendDmRoundTrip",
                target: Some(format!("{}->primary", hex::encode(peer_inbox_bytes))),
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
        assert_eq!(CreateDm.name(), "CreateDm");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "CreateDm",
        depends_on: &["CreateIdentity"],
        make: || Box::new(CreateDm),
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
