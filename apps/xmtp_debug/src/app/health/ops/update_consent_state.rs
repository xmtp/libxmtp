//! Ops: set consent state on every group primary is in.
//!
//! - `UpdateConsentState`: emits an MLS commit recording the new consent.
//! - `UpdateConsentStateQuiet`: updates only local consent state, no commit.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use async_trait::async_trait;
use std::time::Instant;
use xmtp_db::consent_record::ConsentState;

pub struct UpdateConsentState;

#[async_trait]
impl HealthOp for UpdateConsentState {
    fn name(&self) -> &'static str {
        "UpdateConsentState"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateConsentState"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = (|| {
                let group = ctx.primary.group(gid)?;
                group.update_consent_state(ConsentState::Allowed)?;
                Ok(())
            })();
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(format!("{gid}")),
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

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "UpdateConsentStateQuiet")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();
        let db = ctx.primary.db();
        for gid in ctx.all_groups() {
            let start = Instant::now();
            let outcome: color_eyre::eyre::Result<()> = (|| {
                let group = ctx.primary.group(gid)?;
                group.quietly_update_consent_state(ConsentState::Allowed, &db)?;
                Ok(())
            })();
            let (status, error) = match outcome {
                Ok(_) => (Status::Pass, None),
                Err(e) => (Status::Fail, Some(e)),
            };
            out.push(OpResult {
                op_name: self.name(),
                target: Some(format!("{gid}")),
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

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["AddMembersToNewGroup", "AddPrimaryToExistingGroups"],
        op: &UpdateConsentState,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        depends_on: &["UpdateConsentState"],
        op: &UpdateConsentStateQuiet,
        requires: crate::app::health::conditions::Conditions::WRITES,
    }
}
