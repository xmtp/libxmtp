//! Tests for proposal-based group operations.
//!
//! These tests verify:
//! 1. That proposal-based add/remove member flows work correctly
//! 2. That proposal-based group operations work on dictionary-native groups

use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};
use crate::groups::{
    context::XmtpSharedContext,
    intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData},
    send_message_opts::SendMessageOpts,
};
use crate::prelude::*;
use rstest::rstest;
use xmtp_db::{group_intent::IntentKind, prelude::*};
use xmtp_mls_validation::commit::CommitRuleError;

mod edge_cases;
mod missing_proposals;
mod multiple_proposals;
mod proposal_flows;

/// The sender refused the intent before publishing: the typed cause is the
/// publish error, and no receiver processed anything.
pub(super) fn assert_insufficient_permissions_before_publish(error: crate::groups::GroupError) {
    use crate::groups::{GroupError, validated_commit::CommitValidationError};
    let GroupError::Sync(summary) = error else {
        panic!("expected a rejected intent summary, got {error:?}");
    };
    assert!(
        matches!(
            summary.publish_errors.as_slice(),
            [GroupError::CommitValidation(CommitValidationError::Rule(
                CommitRuleError::InsufficientPermissions
            ))]
        ),
        "expected one pre-publish permission rejection, got {summary:?}"
    );
    assert!(summary.process.errored.is_empty());
}

pub(super) fn assert_insufficient_permissions(error: crate::groups::GroupError) {
    use crate::groups::{
        GroupError, mls_sync::GroupMessageProcessingError, validated_commit::CommitValidationError,
    };
    let GroupError::Sync(summary) = error else {
        panic!("expected a rejected intent summary, got {error:?}");
    };
    assert!(matches!(
        summary.other.as_deref(),
        Some(GroupError::ReceiveError(
            GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                CommitRuleError::InsufficientPermissions
            ))
        ))
    ));
    let [(cursor, cause)] = summary.process.errored.as_slice() else {
        panic!("expected one permission rejection, got {summary:?}");
    };
    assert!(cursor.0 > 0);
    assert!(matches!(
        cause,
        GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
            CommitRuleError::InsufficientPermissions
        ))
    ));
    assert!(summary.publish_errors.is_empty());
    assert!(summary.post_commit_errors.is_empty());
}
