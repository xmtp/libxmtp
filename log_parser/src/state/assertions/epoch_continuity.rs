use crate::state::{State, assertions::LogAssertion};
use anyhow::Result;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use xmtp_common::Event;

pub struct EpochContinuityAssertion;

impl LogAssertion for EpochContinuityAssertion {
    fn assert(state: &State) -> Result<()> {
        let groups = state.groups.lock();

        for (group_id, group) in &*groups {
            // Group the events into epochs
            let mut all_group_epochs = state.grouped_epochs.lock();
            let group_epochs = all_group_epochs.entry(group_id.clone()).or_default();

            // Massage the epoch # and auth forward through group states.

            let mut epoch = None;
            let mut auth: Option<String> = None;
            let mut group = group.lock();
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
                        let description = format!("Epoch auth changed mid-epoch. From {a} to {b}");
                        auth = Some(b.clone());
                        state.event.problems.lock().push(description);
                    }
                    _ => {}
                }
            }

            // Group the events into epochs.
            for state in &group.states {
                let state_lock = state.lock();
                let Some(epoch) = state_lock.epoch else {
                    continue;
                };
                let installation_id = state_lock.event.installation.clone();
                drop(state_lock);

                let installation_epochs = group_epochs.entry(installation_id).or_default();
                let installation_epoch = installation_epochs.epochs.entry(epoch).or_default();

                installation_epoch.states.push(state.clone());
            }
        }

        // Add the important non-group events.
        let mut outer_events = HashMap::new();
        for (_group_id, installation) in &mut *state.grouped_epochs.lock() {
            for (installation_id, epochs) in installation {
                let outer_events = outer_events
                    .entry(installation_id.to_string())
                    .or_insert_with(|| {
                        let mut events = vec![];
                        if let Some(client) = state.clients.lock().get(installation_id) {
                            for event in &client.lock().events {
                                match event.event {
                                    Event::ClientCreated
                                    | Event::ClientDropped
                                    | Event::StreamOpened
                                    | Event::StreamClosed => events.push(event.clone()),
                                    _ => {}
                                }
                            }
                        }
                        Arc::new(Mutex::new(events))
                    });

                epochs.outer_events = outer_events.clone();
            }
        }

        Ok(())
    }
}
