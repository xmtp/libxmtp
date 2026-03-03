use crate::state::{State, StateOrEvent, assertions::LogAssertion};
use anyhow::Result;

pub struct BuildTimeline;

impl LogAssertion for BuildTimeline {
    fn assert(state: &State) -> Result<()> {
        let groups = state.groups.lock();

        // Collect the states
        for group in groups.values() {
            let mut group = group.lock();

            let mut timeline = Vec::new();
            for state_map in &group.states {
                for state in state_map.values() {
                    timeline.push(StateOrEvent::State(state.clone()));
                }
            }

            group.timeline = timeline;
            group.timeline.sort();
        }

        Ok(())
    }
}
