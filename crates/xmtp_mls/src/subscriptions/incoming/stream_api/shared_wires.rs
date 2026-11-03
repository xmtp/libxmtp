//! The process registry of native shared Bidi transports, and the suspension
//! intent that every new transport inherits.
//!
//! Clients share a wire only with the same API client Arc, host, and admitted limits.
//! Each client factory owns its transport. The registry keeps only weak handles.
//! A live transport retains the API client, so its address cannot be reused.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Weak};

use parking_lot::Mutex;

use xmtp_api_backend::{
    BackendBinding, BidiConnection, BidiTransport, MutateLimits, OpenError, TransportError,
};
use xmtp_proto::api_client::XmtpMlsBidiStreams;

/// Stable identity of the Arc that owns a backend API client.
pub trait ApiClientIdentity {
    /// The address of the owning Arc, the same for every clone of it.
    fn api_client_identity(&self) -> usize;
}

impl<C: ?Sized> ApiClientIdentity for Arc<C> {
    fn api_client_identity(&self) -> usize {
        Arc::as_ptr(self).cast::<()>() as usize
    }
}

type WireKey = (String, usize, MutateLimits);

struct SharedWires {
    transports: HashMap<WireKey, Weak<BidiTransport<BackendBinding>>>,
    suspend_requested: bool,
}

static SHARED_WIRES: LazyLock<Mutex<SharedWires>> = LazyLock::new(|| {
    Mutex::new(SharedWires {
        transports: HashMap::new(),
        suspend_requested: false,
    })
});

/// Prevent automatic unary receipt from bypassing native stream suspension.
pub(in crate::subscriptions::incoming) fn bidi_streams_suspended() -> bool {
    SHARED_WIRES.lock().suspend_requested
}

/// How many shared router transports are still alive.
#[cfg(any(test, feature = "test-utils"))]
pub fn shared_transport_count() -> usize {
    let mut wires = SHARED_WIRES.lock();
    wires
        .transports
        .retain(|_, transport| transport.strong_count() > 0);
    wires.transports.len()
}

/// The shared transport for the destination `api` dials, created at that
/// destination's first stream (see the module docs and [`SHARED_WIRES`]).
///
/// A transport's ledger task is bound to the async runtime alive at its
/// first use — the one-runtime-per-process reality of mobile, node, and
/// agents. A process that tears its runtime down and starts another (some
/// non-`nextest` test harnesses) would find the cached transport dead;
/// `nextest`'s process-per-test model keeps tests clear of that.
#[cfg(test)]
pub(crate) fn shared_transport<C>(api: C) -> Arc<BidiTransport<BackendBinding>>
where
    C: XmtpMlsBidiStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    let mutate = MutateLimits::from_limits(&api.bidi_limits());
    shared_transport_within(api, mutate)
}

pub(in crate::subscriptions::incoming) fn shared_transport_within<C>(
    api: C,
    mutate: MutateLimits,
) -> Arc<BidiTransport<BackendBinding>>
where
    C: XmtpMlsBidiStreams + ApiClientIdentity + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    // Chunk and pace below ApiClientWrapper with the owning client's snapshot.
    // An API Arc can serve clients admitted with different limits.
    let key = (api.host().to_owned(), api.api_client_identity(), mutate);
    let mut wires = SHARED_WIRES.lock();
    // A transport created while the app is backgrounded is born suspended —
    // its first lease parks instead of dialing (see [`SharedWires`]).
    let born_suspended = wires.suspend_requested;
    wires
        .transports
        .retain(|_, transport| transport.strong_count() > 0);
    if let Some(transport) = wires.transports.get(&key).and_then(Weak::upgrade) {
        return transport;
    }
    tracing::info!(
        host = %key.0,
        suspended = born_suspended,
        "bidi: initializing the shared transport for a destination"
    );
    let transport = Arc::new(BidiTransport::new_within(
        move |initial| {
            let api = api.clone();
            async move {
                BidiConnection::open(&api, initial)
                    .await
                    .map_err(OpenError::new)
            }
        },
        born_suspended,
        mutate,
    ));
    wires.transports.insert(key, Arc::downgrade(&transport));
    transport
}

/// Suspend every shared wire and remember the intent for wires created
/// later. Returns after every live wire acknowledges, with the first failure
/// other than a closed wire.
pub async fn suspend() -> Result<(), TransportError> {
    // Enqueue each transport's command UNDER the same lock that flips the
    // process flag, so a concurrent resume can't interleave its command send
    // between our flag flip and ours — the actor then sees Suspend/Resume in
    // the order the lock ordered the flags, not in scheduler order. The push is
    // synchronous; only the reply await below runs outside the lock.
    let (wires, pending): (Vec<_>, Vec<_>) = {
        let mut shared = SHARED_WIRES.lock();
        shared.suspend_requested = true;
        shared
            .transports
            .iter()
            .filter_map(|(host, weak)| {
                weak.upgrade().map(|transport| {
                    let pending = transport.enqueue_suspend();
                    ((host.clone(), transport), pending)
                })
            })
            .unzip()
    };
    settle_lifecycle(&wires, await_lifecycle_acks(pending).await)
}

/// Resume shared wires and clear the intent for wires created later.
/// Reconnect and target processing continue in their tasks.
pub fn resume() {
    // Enqueue under the flag lock, same as [`suspend`]: the
    // command order must match the flag order under all interleavings. The
    // reply receivers are dropped — fire-and-forget — and an `Err` from a
    // closed (tombstoned) transport is dropped with them: a destination
    // permanently off the network is vacuously resumed.
    let mut shared = SHARED_WIRES.lock();
    shared.suspend_requested = false;
    shared.transports.retain(|_, weak| {
        if let Some(transport) = weak.upgrade() {
            let _ = transport.enqueue_resume();
            true
        } else {
            false
        }
    });
}

/// Await every enqueued lifecycle reply, mapping a dropped sender (a closed
/// transport) to `Closed`. The command pushes already happened under the
/// registry lock ([`suspend`]); only this await runs outside it,
/// so a slow catch-up on one destination never delays another's command.
async fn await_lifecycle_acks(
    pending: Vec<Result<tokio::sync::oneshot::Receiver<()>, TransportError>>,
) -> Vec<Result<(), TransportError>> {
    futures::future::join_all(pending.into_iter().map(|reply| async move {
        match reply {
            Ok(rx) => rx.await.map_err(|_| TransportError::Closed),
            Err(e) => Err(e),
        }
    }))
    .await
}

/// Wait for every wire before reporting a lifecycle failure.
fn settle_lifecycle(
    wires: &[(WireKey, Arc<BidiTransport<BackendBinding>>)],
    results: Vec<Result<(), TransportError>>,
) -> Result<(), TransportError> {
    let mut first_error = None;
    for (_, result) in wires.iter().zip(results) {
        match result {
            Ok(()) => {}
            Err(TransportError::Closed) => {}
            Err(e) => first_error = first_error.or(Some(e)),
        }
    }
    match first_error {
        None => Ok(()),
        Some(e) => Err(e),
    }
}
