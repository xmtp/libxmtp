use crate::state::{State, StateOrEvent, assertions::LogAssertion};
use anyhow::Result;
use std::collections::HashMap;

pub struct BuildTimeline;

impl LogAssertion for BuildTimeline {
    fn assert(state: &State) -> Result<()> {
        let groups = state.groups.lock();
        let mut timeline: HashMap<String, Vec<StateOrEvent>> = HashMap::new();

        // Collect the states
        for (group_id, group) in &*groups {
            let group = group.lock();
            let group_tl = timeline.entry(group_id.clone()).or_default();

            for state in &group.states {
                group_tl.push(StateOrEvent::State(state.clone()));
            }
        }

        // Sort the states
        for (_group_id, states) in &mut timeline {
            states.sort();
        }

        *state.timeline.lock() = timeline;

        Ok(())
    }
}
