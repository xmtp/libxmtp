use crate::state::{assertions::LogAssertion, LogState, StateOrEvent};
use anyhow::Result;
use std::collections::HashMap;

pub struct BuildTimeline;

impl LogAssertion for BuildTimeline {
    fn assert(state: &LogState) -> Result<()> {
        let group_org = state.org_group();
        let mut timeline: HashMap<String, Vec<StateOrEvent>> = HashMap::new();

        // Collect the states
        for (group_id, inst_groups) in &group_org {
            let group_tl = timeline.entry(group_id.clone()).or_default();

            for inst_group in inst_groups {
                let inst_group = inst_group.lock();
                for state in &inst_group.states {
                    group_tl.push(StateOrEvent::State(state.clone()));
                }
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
