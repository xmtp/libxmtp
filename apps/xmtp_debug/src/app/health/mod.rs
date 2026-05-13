//! `xdbg healthcheck` — cross-version libxmtp protocol exerciser.
//!
//! Runs every user-visible op against existing xdbg state, validates
//! convergence, exits non-zero on any failure. See
//! `docs/superpowers/specs/2026-05-13-xdbg-healthcheck-design.md`.

mod context;
pub use context::HealthContext;
mod ops;
mod result;
mod validators;

use crate::args;
use color_eyre::eyre::Result;

pub struct Health {
    #[allow(dead_code)]
    opts: args::HealthcheckOpts,
    #[allow(dead_code)]
    network: args::BackendOpts,
}

impl Health {
    pub fn new(opts: args::HealthcheckOpts, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        // Filled in by Task 4.
        unimplemented!("healthcheck run loop not yet implemented")
    }
}
