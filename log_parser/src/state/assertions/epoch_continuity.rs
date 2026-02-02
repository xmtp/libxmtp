use std::{cmp::Ordering, collections::HashMap};

use crate::state::{
    GroupStateExt, LogState,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;

pub struct EpochContinuityAssertion;

impl LogAssertion for EpochContinuityAssertion {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>> {
        let mut group_collection = HashMap::new();
        for (inst, state) in &state.clients {
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
                    let state_write = state.read();
                    let epoch = state_write.epoch.expect("This is filtered out above.");
                    let group_epoch = group_epochs.entry(epoch).or_default();
                    let installation_group_epoch = group_epoch
                        .states
                        .entry(state_write.installation_id.clone())
                        .or_default();
                    installation_group_epoch.push(state.clone());
                }
            }
        }

        Ok(None)
    }
}
