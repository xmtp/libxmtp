use crate::state::{State, assertions::LogAssertion};
use anyhow::Result;
use std::sync::atomic::Ordering;

pub struct FlagGroupsForErrors;

impl LogAssertion for FlagGroupsForErrors {
    fn assert(state: &State) -> Result<()> {
        let groups = state.groups.lock();

        for (_group_id, group) in &*groups {
            let group = group.lock();

            for event in &*group.timeline {
                if !event.event().problems.lock().is_empty() {
                    group.has_errors.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        Ok(())
    }
}
