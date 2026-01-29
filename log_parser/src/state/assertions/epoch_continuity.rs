use std::{cmp::Ordering, collections::HashMap, sync::Weak};

use crate::state::{
    GroupStateExt, LogState,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;

struct EpochContinuityAssertion;

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
            let groups = groups
                .into_iter()
                .map(|g| {
                    g.traverse().filter_map(|g| {
                        let g = g.read();
                        g.epoch?;
                        Some(g)
                    })
                })
                .collect::<Vec<_>>();

            for group in groups {
                for iteration in group.traverse().filter_map(|g| {
                    let g = g.read();
                    g.epoch?;
                    Some(g)
                }) {}
                for g in group.read().clone().into_iter() {}
                while group.read().epoch.is_none() {
                    if let Some(g) = group.read().next.as_ref().and_then(Weak::upgrade) {
                        group = g;
                    }
                }
            }
        }

        Ok(None)
    }
}
