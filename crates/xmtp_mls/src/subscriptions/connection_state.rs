//! One connection state for the app streams of one client.

use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use xmtp_events::{ClientEvent, ConnectionState, ConnectionStateChanged, EventWriter};

use super::incoming::IncomingConnection;

#[derive(Default)]
pub(crate) struct ConnectionStates {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    streams: HashMap<u64, ConnectionState>,
    current: Option<ConnectionState>,
    writer: Option<Arc<dyn EventWriter<()>>>,
}

impl ConnectionStates {
    pub(crate) fn open_ids(&self) -> Vec<u64> {
        self.inner.lock().streams.keys().copied().collect()
    }

    pub(crate) fn set_writer(&self, writer: Arc<dyn EventWriter<()>>) {
        self.inner.lock().writer = Some(writer);
    }

    pub(crate) fn open(&self, id: u64) {
        self.change(|streams| {
            streams.insert(id, ConnectionState::Connecting);
        });
    }

    pub(crate) fn update(&self, id: u64, state: IncomingConnection) {
        self.change(|streams| {
            if state == IncomingConnection::Closed {
                streams.remove(&id);
                return;
            }
            if let Some(current) = streams.get_mut(&id) {
                *current = match state {
                    IncomingConnection::Connecting => ConnectionState::Connecting,
                    IncomingConnection::Connected => ConnectionState::Connected,
                    IncomingConnection::Reconnecting => ConnectionState::Reconnecting,
                    IncomingConnection::Failed => ConnectionState::Failed,
                    IncomingConnection::Closed => unreachable!(),
                };
            }
        });
    }

    pub(crate) fn close(&self, id: u64) {
        self.change(|streams| {
            streams.remove(&id);
        });
    }

    fn change(&self, change: impl FnOnce(&mut HashMap<u64, ConnectionState>)) {
        let mut inner = self.inner.lock();
        change(&mut inner.streams);
        let next = if inner.streams.is_empty() {
            ConnectionState::Closed
        } else if inner
            .streams
            .values()
            .any(|state| *state == ConnectionState::Failed)
        {
            ConnectionState::Failed
        } else if inner
            .streams
            .values()
            .any(|state| *state == ConnectionState::Reconnecting)
        {
            ConnectionState::Reconnecting
        } else if inner
            .streams
            .values()
            .any(|state| *state == ConnectionState::Connecting)
        {
            ConnectionState::Connecting
        } else {
            ConnectionState::Connected
        };
        let previous = inner.current.unwrap_or(ConnectionState::Closed);
        inner.current = Some(next);
        if previous != next
            && let Some(writer) = &inner.writer
        {
            writer.emit(
                Some(ClientEvent::ConnectionStateChanged(
                    ConnectionStateChanged {
                        previous,
                        current: next,
                    },
                )),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_events::{EventBus, EventFilter, EventKind, PublicBusWriter};

    // verifies: EVENT-001, EVENT-027
    #[xmtp_common::test(unwrap_try = true)]
    fn aggregate_prioritizes_failure_and_emits_only_transitions() {
        let bus = EventBus::<()>::new();
        let received = bus.subscribe(
            EventFilter::new([EventKind::ConnectionStateChanged]),
            Some(16),
        );
        let states = ConnectionStates::default();
        states.set_writer(Arc::new(PublicBusWriter::new(&bus)));
        states.open(1);
        states.update(1, IncomingConnection::Connected);
        states.open(2);
        states.update(2, IncomingConnection::Connected);
        states.update(1, IncomingConnection::Reconnecting);
        states.update(2, IncomingConnection::Failed);
        states.update(2, IncomingConnection::Failed);
        states.close(2);
        states.update(1, IncomingConnection::Connected);
        states.close(1);
        let transitions: Vec<_> = received
            .drain()
            .into_iter()
            .filter_map(|item| match item.client {
                Some(ClientEvent::ConnectionStateChanged(change)) => {
                    Some((change.previous, change.current))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            transitions,
            [
                (ConnectionState::Closed, ConnectionState::Connecting),
                (ConnectionState::Connecting, ConnectionState::Connected),
                (ConnectionState::Connected, ConnectionState::Connecting),
                (ConnectionState::Connecting, ConnectionState::Connected),
                (ConnectionState::Connected, ConnectionState::Reconnecting),
                (ConnectionState::Reconnecting, ConnectionState::Failed),
                (ConnectionState::Failed, ConnectionState::Reconnecting),
                (ConnectionState::Reconnecting, ConnectionState::Connected),
                (ConnectionState::Connected, ConnectionState::Closed),
            ]
        );
    }
}
