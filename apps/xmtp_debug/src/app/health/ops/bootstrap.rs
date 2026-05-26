//! Op: record that the run's primary identity was successfully created.
//! The actual creation happens during `HealthContext::bootstrap`; this op
//! exposes that success as a discrete check in the run's result table.

use crate::app;
use crate::app::health::context::{HealthClient, HealthContext};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use crate::app::store::Database;
use crate::app::types::Identity;
use async_trait::async_trait;
use color_eyre::eyre::Result;
use std::time::Instant;

pub struct BootstrapIdentities;

/// Number of identities to create when the redb identity store is empty.
/// 3 is the minimum that lets us exercise group ops (one creator + two
/// others) and DM ops (primary ↔ peer with at least one extra identity).
const BOOTSTRAP_IDENTITY_COUNT: usize = 3;

#[async_trait]
impl HealthOp for BootstrapIdentities {
    fn name(&self) -> &'static str {
        "BootstrapIdentities"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "BootstrapIdentities")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let start = Instant::now();
        let outcome: Result<()> = async {
            let mut fresh_identities: Vec<Identity> = Vec::new();
            for _ in 0..BOOTSTRAP_IDENTITY_COUNT {
                let wallet = app::generate_wallet();
                let client = app::new_unregistered_client(Some(&wallet)).await?;
                let identity = Identity::from_libxmtp(client.identity(), wallet.clone())?;
                app::register_client(&client, wallet.into_alloy()).await?;
                fresh_identities.push(identity);
                ctx.existing_clients.insert(HealthClient::new(identity));
            }
            ctx.id_store.set_all(fresh_identities.as_slice())?;
            Ok(())
        }
        .await;

        vec![OpResult::from_result(self.name(), None, start, outcome)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(BootstrapIdentities.name(), "BootstrapIdentities");
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &[],
        op: &BootstrapIdentities,
        requires: crate::app::health::conditions::Conditions::BOOTSTRAP
            .union(crate::app::health::conditions::Conditions::WRITES),
    }
}
