//! Ops: set consent state on every group primary is in.
//!
//! - `UpdateConsentState`: emits an MLS commit recording the new consent.
//! - `UpdateConsentStateQuiet`: updates only local consent state, no commit.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::time::Instant;
use xmtp_db::consent_record::ConsentState;

pub struct UpdateConsentState;

#[async_trait]
impl HealthOp for UpdateConsentState {
    fn name(&self) -> &'static str {
        "UpdateConsentState"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = (|| {
                let group = ctx.primary.group(gid).map_err(|e| eyre!("{e}"))?;
                group
                    .update_consent_state(ConsentState::Allowed)
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            })();
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(gid)),
                status,
                duration: start.elapsed(),
                error,
            });
        }
        out
    }
}

pub struct UpdateConsentStateQuiet;

#[async_trait]
impl HealthOp for UpdateConsentStateQuiet {
    fn name(&self) -> &'static str {
        "UpdateConsentStateQuiet"
    }

    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        let db = ctx.primary.db();
        for gid in &all_groups {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = (|| {
                let group = ctx.primary.group(gid).map_err(|e| eyre!("{e}"))?;
                group
                    .quietly_update_consent_state(ConsentState::Allowed, &db)
                    .map_err(|e| eyre!("{e}"))?;
                Ok(())
            })();
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(hex::encode(gid)),
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
    fn names_are_stable() {
        assert_eq!(UpdateConsentState.name(), "UpdateConsentState");
        assert_eq!(UpdateConsentStateQuiet.name(), "UpdateConsentStateQuiet");
    }
}
