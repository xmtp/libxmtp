pub mod epoch_auth_consistency;
pub mod epoch_continuity;

use crate::state::LogState;
use anyhow::Result;

pub trait LogAssertion {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>>;
}

pub struct AssertionFailure {}
