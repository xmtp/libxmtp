use crate::state::{
    GroupStateExt, LogState,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;

struct EpochContinuityAssertion;

impl LogAssertion for EpochContinuityAssertion {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>> {
        let mut groups = vec![];
        for group_id in &state.groups {
            for (installation, client_state) in &state.clients {
                let Some(group) = client_state.groups.get(group_id) else {
                    continue;
                };
                groups.push(group.beginning()?);
            }
        }

        Ok(None)
    }
}
