use crate::state::{
    LogState,
    assertions::{AssertionFailure, LogAssertion},
};
use anyhow::Result;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};
use xmtp_common::Event;

pub struct EpochContinuityAssertion;

impl LogAssertion for EpochContinuityAssertion {
    fn assert(state: &LogState) -> Result<Option<AssertionFailure>> {
        // The value is groups from multiple installations
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
            // Group the events into epochs
            let mut all_group_epochs = state.grouped_epochs.write();
            let group_epochs = all_group_epochs.entry(group_id.clone()).or_default();

            // Massage the epoch # and auth forward through group states.
            for group in &groups {
                let mut epoch = None;
                let mut auth: Option<String> = None;
                let mut group = group.write();
                for state in &mut group.states {
                    let mut state = state.lock();
                    match (epoch, state.epoch) {
                        (Some(e), None) => {
                            state.epoch = Some(e);
                        }
                        (Some(a), Some(b)) if b < a => state
                            .event
                            .problems
                            .lock()
                            .push(format!("Epoch went backwards. Was {a}, is now {b}.")),
                        (_, Some(e)) => {
                            epoch = Some(e);
                            auth = None;
                        }
                        _ => {}
                    }

                    match (&auth, &state.epoch_auth) {
                        (Some(a), None) => state.epoch_auth = Some(a.clone()),
                        (None, Some(a)) => auth = Some(a.clone()),
                        (Some(a), Some(b)) if a != b => {
                            let description =
                                format!("Epoch auth changed mid-epoch. From {a} to {b}");
                            auth = Some(b.clone());
                            state.event.problems.lock().push(description);
                        }
                        _ => {}
                    }
                }
            }

            // Group the events into epochs.
            for group in &groups {
                let group = group.read();

                let installation_epochs = group_epochs
                    .entry(group.installation_id.clone())
                    .or_default();
                for state in &group.states {
                    let Some(epoch) = state.lock().epoch else {
                        continue;
                    };
                    let installation_epoch = installation_epochs.epochs.entry(epoch).or_default();

                    installation_epoch.states.push(state.clone());
                }
            }
        }

        // Add the important non-group events.
        let mut outer_events = HashMap::new();
        for (_group_id, installation) in &mut *state.grouped_epochs.write() {
            for (installation_id, epochs) in installation {
                let outer_events = outer_events
                    .entry(installation_id.to_string())
                    .or_insert_with(|| {
                        let mut events = vec![];
                        if let Some(client) = state.clients.read().get(installation_id) {
                            for event in &client.read().events {
                                match event.event {
                                    Event::ClientCreated
                                    | Event::ClientDropped
                                    | Event::StreamOpened
                                    | Event::StreamClosed => events.push(event.clone()),
                                    _ => {}
                                }
                            }
                        }
                        Arc::new(RwLock::new(events))
                    });

                epochs.outer_events = outer_events.clone();
            }
        }

        Ok(None)
    }
}
