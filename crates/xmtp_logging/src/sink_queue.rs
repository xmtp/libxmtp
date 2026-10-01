//! One queue and one outstanding delivery across all sink generations.

use crate::LogRecord;
use futures::task::AtomicWaker;
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::Arc,
    task::{Context, Poll},
};

/// Maximum records waiting for host admission, including transport.
pub const SINK_QUEUE_CAPACITY: usize = 4096;

struct InFlight {
    generation: u64,
    admitted: bool,
}

struct State<T: ?Sized> {
    generation: u64,
    target: Option<Arc<T>>,
    queue: VecDeque<LogRecord>,
    pending_drops: u64,
    dropped: u64,
    errors: u64,
    flight: Option<InFlight>,
}

/// A dispatched record keeps its queue credit until the host admits it.
pub struct SinkDispatch<T: ?Sized> {
    pub target: Arc<T>,
    pub record: LogRecord,
    generation: u64,
    reported_drops: u64,
}

/// Admission is synchronous and never calls application code.
pub struct SinkQueue<T: ?Sized> {
    state: Mutex<State<T>>,
    ready: AtomicWaker,
}

impl<T: ?Sized> Default for SinkQueue<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                generation: 0,
                target: None,
                queue: VecDeque::new(),
                pending_drops: 0,
                dropped: 0,
                errors: 0,
                flight: None,
            }),
            ready: AtomicWaker::new(),
        }
    }
}

impl<T: ?Sized> SinkQueue<T> {
    // implements: LOG-007, LOG-011, LOG-012
    pub fn replace(&self, target: Option<Arc<T>>) {
        let old = {
            let mut state = self.state.lock();
            state.generation = state
                .generation
                .checked_add(1)
                .expect("log generation exhausted");
            state.queue.clear();
            state.pending_drops = 0;
            std::mem::replace(&mut state.target, target)
        };
        // Foreign destructors can emit logs or replace the sink.
        drop(old);
        self.ready.wake();
    }

    // implements: LOG-003, LOG-004, LOG-005
    pub fn push(&self, record: LogRecord) {
        let mut state = self.state.lock();
        if state.target.is_none() {
            return;
        }
        let transport = usize::from(state.flight.as_ref().is_some_and(|call| !call.admitted));
        if state.queue.len() + transport >= SINK_QUEUE_CAPACITY {
            state.pending_drops = state.pending_drops.saturating_add(1);
            state.dropped = state.dropped.saturating_add(1);
            return;
        }
        state.queue.push_back(record);
        drop(state);
        self.ready.wake();
    }

    // implements: LOG-002, LOG-013
    pub fn poll_next(&self, cx: &mut Context<'_>) -> Poll<SinkDispatch<T>> {
        self.ready.register(cx.waker());
        let mut state = self.state.lock();
        if state.flight.is_some() {
            return Poll::Pending;
        }
        let Some(target) = state.target.clone() else {
            return Poll::Pending;
        };
        let Some(mut record) = state.queue.pop_front() else {
            return Poll::Pending;
        };
        let generation = state.generation;
        let reported_drops = state.pending_drops;
        record.dropped_records = reported_drops;
        state.flight = Some(InFlight {
            generation,
            admitted: false,
        });
        Poll::Ready(SinkDispatch {
            target,
            record,
            generation,
            reported_drops,
        })
    }

    /// Called by the host at its final admission check, before application code.
    // implements: LOG-007
    pub fn handoff(&self) -> bool {
        let mut state = self.state.lock();
        let generation = state.generation;
        let Some(call) = state.flight.as_mut() else {
            return false;
        };
        if call.generation != generation || call.admitted {
            return false;
        }
        call.admitted = true;
        true
    }

    // implements: LOG-009, LOG-012, LOG-013
    pub fn complete(&self, dispatch: SinkDispatch<T>, success: bool) {
        let mut state = self.state.lock();
        if dispatch.generation == state.generation {
            if success {
                state.pending_drops = state.pending_drops.saturating_sub(dispatch.reported_drops);
            } else {
                state.errors = state.errors.saturating_add(1);
            }
        }
        state.flight = None;
        drop(state);
        drop(dispatch);
        self.ready.wake();
    }

    pub fn dropped_count(&self) -> u64 {
        self.state.lock().dropped
    }
    pub fn error_count(&self) -> u64 {
        self.state.lock().errors
    }
}

#[cfg(test)]
mod tests;
