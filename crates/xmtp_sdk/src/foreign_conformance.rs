//! Test-only observations of the real foreign-call futures.
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

static IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicU64 = AtomicU64::new(0);
static STARTED: AtomicU64 = AtomicU64::new(0);
static COMPLETED: AtomicU64 = AtomicU64::new(0);
static DROPPED_EARLY: AtomicU64 = AtomicU64::new(0);
static CALLER_POLLS: AtomicU64 = AtomicU64::new(0);

/// A snapshot for conformance runners, never part of a shipped package.
#[derive(uniffi::Record)]
pub struct SdkConformanceForeignCallCounts {
    pub in_flight: u64,
    pub running: u64,
    pub started: u64,
    pub completed: u64,
    pub dropped_early: u64,
    pub polls_on_caller_thread: u64,
}

/// Private host counter. @xmtp-worker @xmtp-internal
/// Read at a host barrier or after all calls have drained. Fields are atomic,
/// but the snapshot is not a transaction across a running callback.
#[uniffi::export]
pub fn sdk_conformance_foreign_call_counts() -> SdkConformanceForeignCallCounts {
    SdkConformanceForeignCallCounts {
        in_flight: IN_FLIGHT.load(Ordering::SeqCst),
        running: RUNNING.load(Ordering::SeqCst),
        started: STARTED.load(Ordering::SeqCst),
        completed: COMPLETED.load(Ordering::SeqCst),
        dropped_early: DROPPED_EARLY.load(Ordering::SeqCst),
        polls_on_caller_thread: CALLER_POLLS.load(Ordering::SeqCst),
    }
}

pub(crate) fn track<F: Future>(future: F) -> impl Future<Output = F::Output> {
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    Tracked {
        future: Some(Box::pin(future)),
        entered: false,
        complete: false,
        #[cfg(not(target_arch = "wasm32"))]
        caller: std::thread::current().id(),
    }
}

struct Tracked<F: Future> {
    future: Option<Pin<Box<F>>>,
    entered: bool,
    complete: bool,
    #[cfg(not(target_arch = "wasm32"))]
    caller: std::thread::ThreadId,
}

impl<F: Future> Future for Tracked<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.entered {
            self.entered = true;
            STARTED.fetch_add(1, Ordering::SeqCst);
            RUNNING.fetch_add(1, Ordering::SeqCst);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.caller == std::thread::current().id() {
            CALLER_POLLS.fetch_add(1, Ordering::SeqCst);
        }
        let result = self
            .future
            .as_mut()
            .expect("tracked future")
            .as_mut()
            .poll(cx);
        if result.is_ready() {
            self.complete = true;
            COMPLETED.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
}

impl<F: Future> Drop for Tracked<F> {
    fn drop(&mut self) {
        drop(self.future.take());
        if self.entered {
            RUNNING.fetch_sub(1, Ordering::SeqCst);
            if !self.complete {
                DROPPED_EARLY.fetch_add(1, Ordering::SeqCst);
            }
        }
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}
