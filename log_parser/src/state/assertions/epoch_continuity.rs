use std::{cmp::Ordering, collections::HashMap};

use crate::state::{
    GroupStateExt, GroupStateProblem, LogState, Severity,
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
                g.push(group.beginning()?);
            }
        }

        for (group_id, mut groups) in group_collection {
            groups.sort_by(|a, b| {
                let Some(a_epoch) = a.read().epoch else {
                    return Ordering::Less;
                };
                let Some(b_epoch) = b.read().epoch else {
                    return Ordering::Greater;
                };
                a_epoch.cmp(&b_epoch)
            });

            for group in &groups {
                let mut epoch = None;
                for state in group.traverse() {
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
                    g.traverse().filter_map(|g| {
                        {
                            let g_read = g.read();
                            g_read.epoch?;
                        }
                        Some(g)
                    })
                })
                .collect::<Vec<_>>();

            let mut all_group_epochs = state.grouped_epochs.write();
            let group_epochs = all_group_epochs.entry(group_id.clone()).or_default();

            for state_iterator in group_state_iters {
                for state in state_iterator {
                    let mut state_write = state.write();
                    let epoch = state_write.epoch.expect("This is filtered out above.");

                    let installation_epochs = group_epochs
                        .entry(state_write.installation_id.clone())
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
