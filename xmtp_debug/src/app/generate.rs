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
                println!("Groups generated: {} in {} ms", amount, elapsed.as_millis());
                println!("Per-group latency: {:.1} ms", per_op);
                info!("groups generated");
                Ok(())
            }
            Message => {
                let generator = GenerateMessages::new(network, message_opts, *concurrency)?;
                let start = Instant::now();
                let latencies = generator.run(amount).await?;
                let elapsed = start.elapsed();

                // Compute stats from actual send_message latencies (not including sync overhead)
                let total_send_ms: u128 = latencies.iter().map(|d| d.as_millis()).sum();
                let avg_send_ms = if !latencies.is_empty() {
                    total_send_ms as f64 / latencies.len() as f64
                } else {
                    0.0
                };

                println!("Messages sent: {} in {} ms (total with sync overhead)", amount, elapsed.as_millis());
                println!("Per-message send_message() latency: {:.1} ms (excludes sync overhead)", avg_send_ms);
                info!("messages generated");
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
                println!("Identities created: {} in {} ms", amount, elapsed.as_millis());
                println!("Per-identity latency: {:.1} ms", per_op);
                info!("identities generated");
                Ok(())
            }
        }
    }
}
