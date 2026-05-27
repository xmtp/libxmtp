//! Op: primary creates a DM with every existing peer and round-trips one
//! message in each direction.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use futures::FutureExt;
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
        ctx.for_each_existing_client(|ctx, peer_hc| {
            async move {
                let primary_bytes = ctx.primary.inbox_id_bytes();
                let primary_inbox = ctx.primary.inbox_id_hex();
                let peer_inbox_hex = peer_hc.inbox_id_hex();
                let peer_bytes = peer_hc.inbox_id_bytes();
                let mut rows = Vec::with_capacity(2);

                // Direction A: primary → peer. Persist on success.
                let start = Instant::now();
                let dir_a: color_eyre::eyre::Result<()> = async {
                    let primary = ctx.primary()?;
                    let dm = primary
                        .find_or_create_dm(peer_inbox_hex.as_str(), None)
                        .await?;
                    dm.send_message(b"hi from primary", SendMessageOpts::default())
                        .await?;
                    ctx.persist_new_dm(
                        &dm.group_id,
                        primary_bytes,
                        vec![primary_bytes, peer_bytes],
                    );
                    Ok(())
                }
                .await;
                rows.push(OpResult::from_result(
                    "CreateDm",
                    Some(format!("primary->{peer_inbox_hex}")),
                    start,
                    dir_a,
                ));

                // Direction B: peer → primary.
                let start = Instant::now();
                let dir_b: color_eyre::eyre::Result<()> = async {
                    let peer = peer_hc.realize(&ctx.id_store)?;
                    peer.sync_all_welcomes_and_groups(None).await?;
                    let dm = peer.find_or_create_dm(primary_inbox.as_str(), None).await?;
                    dm.send_message(b"hi from peer", SendMessageOpts::default())
                        .await?;
                    Ok(())
                }
                .await;
                rows.push(OpResult::from_result(
                    "SendDmRoundTrip",
                    Some(format!("{peer_inbox_hex}->primary")),
                    start,
                    dir_b,
                ));
                rows
            }
            .boxed()
        })
        .await
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
        depends_on: &["CreateIdentity"],
        op: &CreateDm,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
