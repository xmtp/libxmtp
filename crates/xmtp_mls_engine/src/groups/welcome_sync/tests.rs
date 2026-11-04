use super::*;
use crate::groups::WelcomeMembership;
use crate::groups::test_support::MlsGroupExt;
use crate::groups::test_support::{EnginePeer, EngineTestPeer};
use crate::prelude::*;

use rstest::*;

mod expiry;
mod ordering;
mod pointee_failures;
mod rate_limits;
mod unreadable;
mod unsupported_and_legacy;

struct RejectMembership {
    retryable: bool,
}
