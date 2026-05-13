//! `xdbg healthcheck` — cross-version libxmtp protocol exerciser.
//!
//! Runs every user-visible op against existing xdbg state, validates
//! convergence, exits non-zero on any failure. See
//! `docs/superpowers/specs/2026-05-13-xdbg-healthcheck-design.md`.

mod context;
mod ops;
mod registry;
mod result;
mod validators;

pub use context::HealthContext;

use crate::args;
use color_eyre::eyre::{Result, eyre};
use std::time::Instant;

pub struct Health {
    #[allow(dead_code)]
    opts: args::HealthcheckOpts,
    network: args::BackendOpts,
}

impl Health {
    pub fn new(opts: args::HealthcheckOpts, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        // Print the resolved op execution order before bootstrap so it's
        // visible even if bootstrap fails.
        print!("{}", ops::tree::render_order_tree());

        let mut ctx = HealthContext::bootstrap(self.network).await?;
        let mut report = result::Report::new();

        // Ops phase. Each op's `execute` is annotated with
        // `#[tracing::instrument]` carrying the op name as a span field, so
        // structured log consumers (--json / --logfmt) can correlate events
        // to ops.
        for op in ops::registry() {
            let results = op.execute(&mut ctx).await;
            for r in results {
                r.print();
                report.push(r);
            }
        }

        // Final sync between ops and validators.
        sync_all(&ctx, &mut report).await;

        // Validation phase.
        for v in validators::registry() {
            let results = v.validate(&mut ctx).await;
            for r in results {
                r.print();
                report.push(r);
            }
        }

        report.print_summary();

        if report.has_failures() {
            std::process::exit(1);
        }
        Ok(())
    }
}

async fn sync_all(ctx: &HealthContext, report: &mut result::Report) {
    for client in ctx.all_clients() {
        let start = Instant::now();
        let outcome = client.sync_all_welcomes_and_groups(None).await;
        let (status, error) = match outcome {
            Ok(_) => (result::Status::Pass, None),
            Err(e) => (result::Status::Fail, Some(eyre!("{e}"))),
        };
        let r = result::OpResult {
            op_name: "Sync",
            target: Some(client.inbox_id().to_string()),
            status,
            duration: start.elapsed(),
            error,
        };
        r.print();
        report.push(r);
    }
}
