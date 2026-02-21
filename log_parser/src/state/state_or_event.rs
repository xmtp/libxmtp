use crate::state::{GroupState, LogEvent};
use parking_lot::Mutex;
use std::sync::Arc;

pub enum StateOrEvent {
    State(Arc<Mutex<GroupState>>),
    Event(Arc<LogEvent>),
}

impl Ord for StateOrEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let state;
        let event = match self {
            Self::Event(event) => event,
            Self::State(s) => {
                state = s.lock();
                &state.event
            }
        };

        let other_state;
        let other_event = match other {
            Self::Event(e) => e,
            Self::State(s) => {
                other_state = s.lock();
                &other_state.event
            }
        };

        event.time().cmp(&other_event.time())
    }
}

impl Eq for StateOrEvent {}

impl PartialOrd for StateOrEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let state;
        let event = match self {
            Self::Event(event) => event,
            Self::State(s) => {
                state = s.lock();
                &state.event
            }
        };

        let other_state;
        let other_event = match other {
            Self::Event(e) => e,
            Self::State(s) => {
                other_state = s.lock();
                &other_state.event
            }
        };
        event.partial_cmp(other_event)
    }
}
impl PartialEq for StateOrEvent {
    fn eq(&self, other: &Self) -> bool {
        let state;
        let event = match self {
            Self::Event(event) => event,
            Self::State(s) => {
                state = s.lock();
                &state.event
            }
        };

        let other_state;
        let other_event = match other {
            Self::Event(e) => e,
            Self::State(s) => {
                other_state = s.lock();
                &other_state.event
            }
        };
        event == other_event
    }
}
