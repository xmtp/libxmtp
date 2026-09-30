//! Per-client admission before any backend request.

use crate::{ApiError, Result, dyn_err};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::{api::NetworkError, api_client::XmtpBackendClient, backend_v1 as wire};

mod dispatch;

/// A request failed before its target was sent. A later operation may retry.
#[derive(Clone, Debug, thiserror::Error)]
#[error("request configuration check failed: {cause}")]
pub struct PreflightError {
    #[source]
    cause: Arc<NetworkError>,
}
impl PreflightError {
    pub fn new(cause: impl RetryableError + 'static) -> Self {
        Self {
            cause: Arc::new(NetworkError::new(cause)),
        }
    }
}
impl RetryableError for PreflightError {
    fn is_retryable(&self) -> bool {
        self.cause.is_retryable()
    }
}

/// Find the admission marker without treating a failed configuration RPC as
/// a failed target RPC. Explicitly unwrap boxes that hide their inner source.
pub fn failure<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a PreflightError> {
    if let Some(error) = error.downcast_ref::<PreflightError>() {
        return Some(error);
    }
    if let Some(error) = error.downcast_ref::<Box<ApiError>>() {
        return failure(error.as_ref());
    }
    if let Some(error) = error.downcast_ref::<Box<PreflightError>>() {
        return Some(error.as_ref());
    }
    error.source().and_then(failure)
}

/// Restricted access to the credential-free configuration RPC. The hook
/// cannot use this capability to send the operation it is checking.
#[xmtp_common::async_trait]
pub trait ConfigurationFetch: MaybeSend + MaybeSync {
    async fn fetch(&self) -> Result<wire::GetConfigurationResponse>;
    fn backend_url(&self) -> Option<&str>;
}

#[xmtp_common::async_trait]
pub trait RequestPreflight: MaybeSend + MaybeSync {
    async fn check(
        &self,
        fetch: &dyn ConfigurationFetch,
    ) -> std::result::Result<(), PreflightError>;
}

#[derive(Debug, thiserror::Error)]
#[error("client configuration gate is not bound")]
struct Unbound;
impl RetryableError for Unbound {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Slot {
    hook: OnceLock<Arc<dyn RequestPreflight>>,
    pending: AtomicBool,
}

/// Backend dispatch with a clone-shared client admission hook. The raw
/// transport is private so a caller cannot skip admission through this value.
pub struct GuardedApi<A> {
    raw: Arc<A>,
    slot: Arc<Slot>,
}
impl<A> GuardedApi<A> {
    pub(crate) fn new(raw: A) -> Self {
        Self {
            raw: Arc::new(raw),
            slot: Arc::default(),
        }
    }
    /// Mark the construction interval fail-closed until the client hook binds.
    pub fn require_preflight(&self) {
        self.slot.pending.store(true, Ordering::Release);
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub fn raw_for_test(&self) -> &A {
        &self.raw
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub fn raw_mut_for_test(&mut self) -> Option<&mut A> {
        Arc::get_mut(&mut self.raw)
    }
}
impl<A: std::fmt::Debug> std::fmt::Debug for GuardedApi<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardedApi")
            .field("raw", &self.raw)
            .finish_non_exhaustive()
    }
}
impl<A> Clone for GuardedApi<A> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            slot: self.slot.clone(),
        }
    }
}
struct RawConfiguration<A>(Arc<A>);
#[xmtp_common::async_trait]
impl<A: XmtpBackendClient> ConfigurationFetch for RawConfiguration<A> {
    async fn fetch(&self) -> Result<wire::GetConfigurationResponse> {
        self.0
            .get_configuration(wire::GetConfigurationRequest {})
            .await
            .map_err(dyn_err)
    }
    fn backend_url(&self) -> Option<&str> {
        self.0.backend_url()
    }
}
impl<A: XmtpBackendClient> GuardedApi<A> {
    /// Install the client hook with its restricted configuration capability.
    pub fn bind_preflight(
        &self,
        make: impl FnOnce(Arc<dyn ConfigurationFetch>) -> Arc<dyn RequestPreflight>,
    ) -> bool
    where
        A: 'static,
    {
        self.slot
            .hook
            .set(make(Arc::new(RawConfiguration(self.raw.clone()))))
            .is_ok()
    }
    pub async fn check_preflight(&self) -> Result<()> {
        if let Some(hook) = self.slot.hook.get() {
            hook.check(&RawConfiguration(self.raw.clone()))
                .await
                .map_err(ApiError::Preflight)
        } else if self.slot.pending.load(Ordering::Acquire) {
            Err(ApiError::Preflight(PreflightError::new(Unbound)))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
