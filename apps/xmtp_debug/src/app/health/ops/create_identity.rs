//! Op: generate a fresh wallet, register an MLS client for it on the
//! network, persist the resulting `Identity` to the redb
//! `IdentityStore`, and write the registered `HealthClient` into
//! `ctx.primary`. Bootstrap leaves `ctx.primary` as a local-only
//! placeholder; no op realizes the placeholder before this op runs
//! (we have no deps, so we're first in the writable schedule).
//!
//! Gated on `WRITES` — read-only runs reuse an existing identity
//! picked during `HealthContext::bootstrap` and skip this op entirely.
//!
//! NOTE: reusing the installation keys baked into `bootstrap`'s
//! placeholder via `.identity()` injection on the builder isn't
//! feasible without rebuilding the full SignatureRequest (CreateInbox,
//! install->inbox AddAssociation, install pre-signature). The MLS
//! network rejects every group commit with `InboxValidationFailed`
//! when the install->inbox association is missing. Letting the builder
//! generate its own install keys via `IdentityStrategy::new` is the
//! supported path.

use crate::app;
use crate::app::health::context::{HealthClient, HealthContext};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use crate::app::store::Database;
use crate::app::types::Identity;
use async_trait::async_trait;
use color_eyre::eyre::Result;
use std::time::Instant;

pub struct CreateIdentity;

#[async_trait]
impl HealthOp for CreateIdentity {
    fn name(&self) -> &'static str {
        "CreateIdentity"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "CreateIdentity"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let start = Instant::now();
        let outcome: Result<Identity> = async {
            let wallet = app::generate_wallet();
            let client = app::new_unregistered_client(Some(&wallet)).await?;
            let identity = Identity::from_libxmtp(client.identity(), wallet.clone())?;
            app::register_client(&client, wallet.into_alloy()).await?;
            ctx.id_store.set(identity)?;
            Ok(identity)
        }
        .await;

        match outcome {
            Ok(identity) => {
                let target = Some(hex::encode(identity.inbox_id));
                ctx.primary = HealthClient::new(identity);
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
        assert_eq!(CreateIdentity.name(), "CreateIdentity");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &[],
        op: &CreateIdentity,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
