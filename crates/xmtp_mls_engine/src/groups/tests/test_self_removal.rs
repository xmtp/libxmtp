//! Self-removal and pending-remove bookkeeping.

use super::*;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};

mod leave_rules;
mod pending_remove_list;
mod readd_and_installations;
