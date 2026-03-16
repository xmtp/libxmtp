use crate::{app::App, args};
mod groups;
mod identity;
mod messages;

pub use groups::*;
pub use identity::*;
pub use messages::*;

use color_eyre::eyre::Result;
use std::time::Instant;

#[derive(Debug)]
pub struct Generate {
    opts: args::Generate,
    network: args::BackendOpts,
}

impl Generate {
    pub fn new(opts: args::Generate, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        use args::EntityKind::*;
        let Generate { opts, network } = self;
        let args::Generate {
            entity,
            amount,
            invite,
            message_opts,
            concurrency,
            ryow,
            ..
        } = opts;

        info!(?concurrency, "using concurrency");

        match entity {
            Group => {
                let db = App::db()?;
                let start = Instant::now();
                GenerateGroups::new(db, network)
                    .create_groups(amount, invite.unwrap_or(0), *concurrency)
                    .await?;
                let elapsed = start.elapsed();
                let per_op = elapsed.as_millis() as f64 / amount as f64;
                info!(
                    count = amount,
                    elapsed_ms = elapsed.as_millis() as u64,
                    per_group_ms = format!("{:.1}", per_op),
                    "groups generated"
                );
                Ok(())
            }
            Message => {
                let generator = GenerateMessages::new(network, message_opts, *concurrency)?;
                let start = Instant::now();
                let latencies = generator.run(amount).await?;
                let elapsed = start.elapsed();

                let total_send_ms: u128 = latencies.iter().map(|d| d.as_millis()).sum();
                let avg_send_ms = if !latencies.is_empty() {
                    total_send_ms as f64 / latencies.len() as f64
                } else {
                    0.0
                };

                info!(
                    count = amount,
                    elapsed_ms = elapsed.as_millis() as u64,
                    avg_send_ms = format!("{:.1}", avg_send_ms),
                    "messages sent (elapsed includes sync overhead, avg_send_ms excludes it)"
                );
                Ok(())
            }
            Identity => {
                let db = App::db()?;
                let start = Instant::now();
                GenerateIdentity::new(db.into(), network)
                    .create_identities(amount, *concurrency, ryow)
                    .await?;
                let elapsed = start.elapsed();
                let per_op = elapsed.as_millis() as f64 / amount as f64;
                info!(
                    count = amount,
                    elapsed_ms = elapsed.as_millis() as u64,
                    per_identity_ms = format!("{:.1}", per_op),
                    "identities created"
                );
                Ok(())
            }
        }
    }
}
