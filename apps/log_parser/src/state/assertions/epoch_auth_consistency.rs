use crate::state::{State, assertions::LogAssertion};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub struct EpochAuthConsistency;

impl LogAssertion for EpochAuthConsistency {
    fn assert(state: &State) -> Result<()> {
        let epochs = state.grouped_epochs.lock();

        let mut auths = HashMap::new();

        // First build the set
        for (group_id, inst_epochs) in &*epochs {
            for epochs in inst_epochs.values() {
                for (epoch_id, epoch) in &epochs.epochs {
                    for state in &epoch.states {
                        let state = state.lock();
                        if let Some(auth) = &state.epoch_auth {
                            let auths = auths
                                .entry(format!("{group_id}-{epoch_id}"))
                                .or_insert_with(HashSet::new);
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
            for epochs in inst_epochs.values() {
                for (epoch_id, epoch) in &epochs.epochs {
                    for state in &epoch.states {
                        let state = state.lock();
                        if state.epoch.is_none() {
                            continue;
                        }

                        if let Some(auth) = auths.get(&format!("{group_id}-{epoch_id}"))
                            && auth.len() > 1
                        {
                            state
                                .event
                                .problems
                                .lock()
                                .push("Multiple epoch auths among clients detected.".to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
