//! Common types shared among all XMTP Crates
// required to be able to use xmtp_macros::async_trait in this crate
extern crate self as xmtp_common;

mod macros;

mod error_code;
pub use error_code::ErrorCode;

#[doc(inline)]
pub use xmtp_macro::ErrorCode;

#[cfg(any(test, feature = "test-utils"))]
mod test;
#[cfg(any(test, feature = "test-utils"))]
pub use test::*;

#[doc(inline)]
#[cfg(any(test, feature = "test-utils"))]
pub use xmtp_macro::test;

#[doc(inline)]
pub use xmtp_macro::async_trait;

#[cfg(feature = "bench")]
pub mod bench;

pub mod retry;
pub use retry::*;

pub mod wasm;
pub use wasm::*;

pub mod stream_handles;
pub use stream_handles::*;

pub mod fmt;
pub mod hex;
pub mod http;
pub mod snippet;
pub mod time;
pub mod types;

pub mod r#const;
pub use r#const::*;

mod event_logging;
pub use event_logging::*;

pub use xmtp_cryptography::hash::*;
pub use xmtp_cryptography::rand::*;

/// Run a future with its own Sentry Hub so its breadcrumbs never interleave
/// with concurrent tasks. Near-free when no Sentry client is configured.
///
/// Forks from `Hub::current()`, not `Hub::main()`: the fork inherits the calling
/// scope, so the caller's transaction stays the parent of everything inside (a
/// `main()` fork has no current span and re-roots inner spans as their own
/// transactions).
///
/// A hub is a *snapshot*: a thread's hub is forked off the process hub the first
/// time that thread touches it, so a client bound afterwards never reaches it,
/// and captures on a client-less hub are dropped in silence. The fork therefore
/// re-reads the process hub on every poll and rebinds whenever the client there
/// is not the one it last took. That tracks the whole `enable` / `disable` /
/// `enable` cycle a host can drive over FFI: the second enable installs a
/// *different* client, and a task that had latched onto the first would keep
/// reporting into a closed one for the rest of its (long) life.
#[cfg(not(target_arch = "wasm32"))]
pub fn bind_task_hub<F: core::future::Future>(
    fut: F,
) -> impl core::future::Future<Output = F::Output> {
    use sentry_core::{Hub, SentryFutureExt};
    let hub = std::sync::Arc::new(Hub::new_from_top(Hub::current()));
    AdoptMainClient {
        main: Hub::main(),
        adopted: None,
        inner: fut,
    }
    .bind_hub(hub)
}

/// Rebind `Hub::current()` — the task hub, since this only runs inside the
/// `bind_hub` wrapper — to `main`'s client whenever that is not `adopted`, the
/// client this task last took from there.
///
/// Identity, not presence: latching on the first client seen strands the task on
/// a closed one after a disable/enable cycle. A cleared process hub propagates
/// too, so a disable stops in-flight tasks. A hub whose client was inherited from
/// the fork rather than taken from here keeps it, because a client-less process
/// hub matches an `adopted` of `None`.
#[cfg(not(target_arch = "wasm32"))]
fn adopt_main_client(
    main: &sentry_core::Hub,
    adopted: &mut Option<std::sync::Arc<sentry_core::Client>>,
) {
    let current = main.client();
    match (&current, &*adopted) {
        (None, None) => return,
        (Some(a), Some(b)) if std::sync::Arc::ptr_eq(a, b) => return,
        _ => {}
    }
    sentry_core::Hub::current().bind_client(current.clone());
    *adopted = current;
}

/// Runs [`adopt_main_client`] on the hub `SentryFuture` installs for the poll.
/// Sits *inside* the `bind_hub` wrapper precisely so `Hub::current()` is the task
/// hub while polling.
#[cfg(not(target_arch = "wasm32"))]
struct AdoptMainClient<F> {
    /// The process hub, held rather than re-fetched so the steady-state check is
    /// one read of its stack with no `Hub::main()` refcount traffic on top.
    ///
    /// A generation counter would make that check a single atomic load, but only
    /// `xmtp_logging` knows when the client changes and it cannot call in here:
    /// `xmtp_common` already depends on `xmtp_logging` for the test subscriber,
    /// and cargo rejects the back edge as a cyclic package dependency.
    main: std::sync::Arc<sentry_core::Hub>,
    /// Kept alive so its address stays unique for the `ptr_eq` above.
    adopted: Option<std::sync::Arc<sentry_core::Client>>,
    inner: F,
}

#[cfg(not(target_arch = "wasm32"))]
impl<F: core::future::Future> core::future::Future for AdoptMainClient<F> {
    type Output = F::Output;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        // safe because we consider `inner` to be structurally pinned, and the
        // other fields not
        // https://doc.rust-lang.org/std/pin/#choosing-pinning-to-be-structural-for-field
        let this = unsafe { self.get_unchecked_mut() };
        adopt_main_client(&this.main, &mut this.adopted);
        unsafe { core::pin::Pin::new_unchecked(&mut this.inner) }.poll(cx)
    }
}

/// Identity passthrough: wasm is single-threaded and has no Sentry client.
#[cfg(target_arch = "wasm32")]
pub fn bind_task_hub<F: core::future::Future>(fut: F) -> F {
    fut
}

pub use xmtp_macro::db_span;
pub use xmtp_macro::err_span;
pub use xmtp_macro::log_event;
pub use xmtp_macro::mls_span;
pub use xmtp_macro::rpc_span;
pub use xmtp_macro::span;
pub use xmtp_macro::timeout;
