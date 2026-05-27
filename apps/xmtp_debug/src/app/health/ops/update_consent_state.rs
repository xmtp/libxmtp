//! Ops: set consent state on every group primary is in.
//!
//! - `UpdateConsentState`: emits an MLS commit recording the new consent.
//! - `UpdateConsentStateQuiet`: updates only local consent state, no commit.

use crate::app::health::context::HealthContext;
use crate::app::health::ops::HealthOp;
use crate::app::health::result::OpResult;
use async_trait::async_trait;
use xmtp_db::consent_record::ConsentState;

pub struct UpdateConsentState;

#[async_trait]
impl HealthOp for UpdateConsentState {
    fn name(&self) -> &'static str {
        "UpdateConsentState"
    }

    #[tracing::instrument(target = "healthcheck.op", skip_all, fields(op = "UpdateConsentState"))]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        ctx.for_each_group(self.name(), |primary, gid| async move {
            primary
                .group(&gid)?
                .update_consent_state(ConsentState::Allowed)?;
            Ok(())
        })
        .await
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
        ctx.for_each_group(self.name(), |primary, gid| async move {
            let db = primary.db();
            primary
                .group(&gid)?
                .quietly_update_consent_state(ConsentState::Allowed, &db)?;
            Ok(())
        })
        .await
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
