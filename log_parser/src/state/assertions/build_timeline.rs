use crate::state::{State, StateOrEvent, assertions::LogAssertion};
use anyhow::Result;

pub struct BuildTimeline;

impl LogAssertion for BuildTimeline {
    fn assert(state: &State) -> Result<()> {
        let groups = state.groups.lock();

        // Collect the states
        for (_group_id, group) in &*groups {
            let mut group = group.lock();

            let mut timeline = Vec::new();
            for state_map in &group.states {
                for (_installation_id, state) in state_map {
                    timeline.push(StateOrEvent::State(state.clone()));
                }
            }

            group.timeline = timeline;
            group.timeline.sort();
        }

        Ok(())
    }
}
