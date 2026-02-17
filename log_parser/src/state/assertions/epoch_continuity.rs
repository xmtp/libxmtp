use std::collections::HashMap;

use crate::state::{
    GroupStateProblem, LogState, Severity,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;

pub struct EpochContinuityAssertion;

impl LogAssertion for EpochContinuityAssertion {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>> {
        let mut group_collection = HashMap::new();
        for (_inst, state) in &*state.clients.read() {
            for (group_id, group) in &state.read().groups {
                let g = group_collection
                    .entry(group_id.clone())
                    .or_insert_with(|| vec![]);
                g.push(group.clone());
            }
        }

        for (group_id, groups) in group_collection {
            for group in &groups {
                let mut epoch = None;
                for state in &group.read().states {
                    let mut state = state.write();
                    if let Some(state_epoch) = state.epoch {
                        if let Some(e) = epoch
                            && e > state_epoch
                        {
                            state.problems.push(GroupStateProblem {
                                description: format!(
                                    "Epoch traveled backwards. From {e} to {state_epoch}"
                                ),
                                severity: Severity::Error,
                            });
                        }

                        epoch = Some(state_epoch);

                        continue;
                    }

                    state.epoch = epoch;
                }
            }

            // Check for continuity
            let group_state_iters = groups
                .into_iter()
                .map(|g| {
                    let states = g.read().states.clone();
                    states
                        .into_iter()
                        .filter_map(|g| {
                            {
                                let g_read = g.read();
                                g_read.epoch?;
                            }
                            Some(g.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            let mut all_group_epochs = state.grouped_epochs.write();
            let group_epochs = all_group_epochs.entry(group_id.clone()).or_default();

            for state_iterator in group_state_iters {
                for state in state_iterator {
                    let mut state_write = state.write();
                    let epoch = state_write.epoch.expect("This is filtered out above.");

                    let installation_epochs = group_epochs
                        .entry(state_write.event.installation.clone())
                        .or_default();
                    let installation_epoch = installation_epochs.entry(epoch).or_default();

                    match (&installation_epoch.auth, &state_write.epoch_auth) {
                        (None, Some(auth)) => {
                            installation_epoch.auth = Some(auth.clone());
                        }
                        (Some(a), Some(b)) if a != b => {
                            let description =
                                format!("Epoch auth changed mid-epoch. old: {a}, new: {b}");
                            state_write.problems.push(GroupStateProblem {
                                description,
                                severity: Severity::Error,
                            });
                        }
                        _ => {}
                    }

                    installation_epoch.states.push(state.clone());
                }
            }
        }

        Ok(None)
    }
}
