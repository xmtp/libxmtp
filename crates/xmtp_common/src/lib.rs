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
/// and captures on a client-less hub are dropped in silence. Two things stop
/// tasks bound before `enable_sentry` from being lost that way: the fork adopts
/// `Hub::main()`'s client when it has none of its own, and every poll retries
/// that adoption until it lands — so a long-lived worker bound before any client
/// existed starts reporting on its first poll after `enable_sentry` binds the
/// process hub. Once bound, the per-poll cost is a single bool check.
#[cfg(not(target_arch = "wasm32"))]
pub fn bind_task_hub<F: core::future::Future>(
    fut: F,
) -> impl core::future::Future<Output = F::Output> {
    use sentry_core::{Hub, SentryFutureExt};
    let hub = std::sync::Arc::new(Hub::new_from_top(Hub::current()));
    let bound = adopt_main_client(&hub);
    AdoptMainClient { bound, inner: fut }.bind_hub(hub)
}

/// Give `hub` the process hub's client if it has none. Returns whether `hub` has
/// a client now, i.e. whether there is anything left to retry.
#[cfg(not(target_arch = "wasm32"))]
fn adopt_main_client(hub: &sentry_core::Hub) -> bool {
    if hub.client().is_some() {
        return true;
    }
    let Some(client) = sentry_core::Hub::main().client() else {
        return false;
    };
    hub.bind_client(Some(client));
    true
}

/// Retries [`adopt_main_client`] on the hub `SentryFuture` installs for the poll,
/// until that hub has a client. Sits *inside* the `bind_hub` wrapper precisely so
/// `Hub::current()` is the task hub while polling.
#[cfg(not(target_arch = "wasm32"))]
struct AdoptMainClient<F> {
    bound: bool,
    inner: F,
}

#[cfg(not(target_arch = "wasm32"))]
impl<F: core::future::Future> core::future::Future for AdoptMainClient<F> {
    type Output = F::Output;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        // safe because we consider `inner` to be structurally pinned, and `bound` not
        // https://doc.rust-lang.org/std/pin/#choosing-pinning-to-be-structural-for-field
        let this = unsafe { self.get_unchecked_mut() };
        if !this.bound {
            this.bound = adopt_main_client(&sentry_core::Hub::current());
        }
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
