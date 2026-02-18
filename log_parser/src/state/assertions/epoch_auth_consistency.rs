use crate::state::{
    LogState,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub struct EpochAuthConsistency;

impl LogAssertion for EpochAuthConsistency {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>> {
        let epochs = state.grouped_epochs.read();

        let mut auths = HashMap::new();

        // First build the set
        for (group_id, inst_epochs) in &*epochs {
            for (_inst, epochs) in inst_epochs {
                for (epoch_id, epoch) in &epochs.epochs {
                    for state in &epoch.states {
                        let state = state.lock();
                        if let Some(auth) = &state.epoch_auth {
                            let auths = auths
                                .entry(format!("{group_id}-{epoch_id}"))
                                .or_insert_with(|| HashSet::new());
                            if !auths.contains(auth) {
                                auths.insert(auth.clone());
                            }
                        }
                    }
                }
            }
        }

        // Then check
        for (group_id, inst_epochs) in &*epochs {
            for (_inst, epochs) in inst_epochs {
                for (epoch_id, epoch) in &epochs.epochs {
                    for state in &epoch.states {
                        let state = state.lock();
                        if state.epoch.is_none() {
                            continue;
                        }

                        if let Some(auth) = auths.get(&format!("{group_id}-{epoch_id}")) {
                            if auth.len() > 1 {
                                state
                                    .event
                                    .problems
                                    .lock()
                                    .push(format!("Multiple epoch auths among clients detected."));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}
