//! Admin and permission enforcement for proposals.

use crate::groups::intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData};
use crate::groups::test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps};
use xmtp_db::{group_intent::IntentKind, prelude::*};
use xmtp_mls_core::context::CoreContext;

use super::test_proposals::{
    assert_insufficient_permissions, assert_insufficient_permissions_before_publish,
};

mod proposer_and_committer;
mod registry_actions;
mod removal_validation;
