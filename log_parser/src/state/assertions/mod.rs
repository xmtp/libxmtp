pub mod account_for_drift;
pub mod build_group_order;
pub mod build_timeline;
pub mod epoch_auth_consistency;
pub mod epoch_continuity;

use crate::state::State;
use anyhow::Result;

pub trait LogAssertion {
    fn assert(state: &State) -> Result<()>;
}
