//! `xdbg healthcheck` — cross-version libxmtp protocol exerciser.
//!
//! Runs every user-visible op against existing xdbg state, validates
//! convergence, exits non-zero on any failure. See
//! `docs/superpowers/specs/2026-05-13-xdbg-healthcheck-design.md`.

mod conditions;
mod context;
mod ops;
mod registry;
mod result;
mod validators;

pub use context::{HealthContext, inbox_id_to_bytes};

use crate::args;
use color_eyre::eyre::{Result, eyre};
use std::time::Instant;

pub struct Health {
    #[allow(dead_code)]
    opts: args::HealthcheckOpts,
    network: args::BackendOpts,
    strict_versioning: bool,
}

impl Health {
    pub fn new(
        opts: args::HealthcheckOpts,
        network: args::BackendOpts,
        strict_versioning: bool,
    ) -> Self {
        Self {
            opts,
            network,
            strict_versioning,
        }
    }

    pub async fn run(self) -> Result<()> {
        // Print the resolved op execution order before bootstrap so it's
        // visible even if bootstrap fails.
        print!("{}", ops::tree::render_order_tree());

        let mut ctx =
            HealthContext::bootstrap(self.network, self.strict_versioning, self.opts.read_only)
                .await?;
        let mut report = result::Report::new();

        let active = conditions::Conditions::active(self.strict_versioning, self.opts.read_only);
        let op_build = ops::registry(active);

        // Surface skipped ops up-front so the operator sees the
        // complete picture before any work runs.
        op_build
            .skipped
            .iter()
            .map(|s| result::OpResult::skipped(s.name, s.missing))
            .for_each(|r| record_result(&mut report, r));

        // Ops phase. Each op's `execute` is annotated with
        // `#[tracing::instrument]` carrying the op name as a span field, so
        // structured log consumers (--json / --logfmt) can correlate events
        // to ops.
        for op in op_build.items {
            for r in op.execute(&mut ctx).await {
                record_result(&mut report, r);
            }
        }

        // Final sync between ops and validators.
        sync_all(&ctx, &mut report).await;

        // Validation phase.
        let validator_build = validators::registry(active);
        validator_build
            .skipped
            .iter()
            .map(|s| result::OpResult::skipped(s.name, s.missing))
            .for_each(|r| record_result(&mut report, r));
        for v in validator_build.items {
            for r in v.validate(&mut ctx).await {
                record_result(&mut report, r);
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
        record_result(report, r);
    }
}

/// Print + push into the report. Centralizes the two-step pattern
/// repeated for every op/validator result on v1.10. (v1.10's
/// `OpResult` doesn't have an `emit()` method, so this is print+push;
/// `main` has print+emit+push.)
fn record_result(report: &mut result::Report, r: result::OpResult) {
    r.print();
    report.push(r);
}
