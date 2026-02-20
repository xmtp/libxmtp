use crate::state::{LogState, StateOrEvent, assertions::LogAssertion};
use anyhow::Result;
use std::collections::HashMap;

pub struct AccountForDrift;

impl LogAssertion for AccountForDrift {
    fn assert(state: &LogState) -> Result<()> {
        let sources = state.sources.lock();
    }
}
