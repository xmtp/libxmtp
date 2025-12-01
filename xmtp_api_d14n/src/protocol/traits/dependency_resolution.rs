use std::collections::HashSet;

use derive_builder::UninitializedFieldError;
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::api::BodyError;

use crate::protocol::{CursorStoreError, Envelope, EnvelopeError, types::MissingEnvelope};

pub struct Resolved<E> {
    pub resolved: Vec<E>,
    /// list of envelopes that could not be resolved with this strategy
    pub unresolved: Option<HashSet<MissingEnvelope>>,
}

impl<E> Resolved<E> {
    pub fn new(envelopes: Vec<E>, unresolved: Option<HashSet<MissingEnvelope>>) -> Self {
        Self {
            resolved: envelopes,
            unresolved,
        }
    }
}

#[xmtp_common::async_trait]
pub trait ResolveDependencies: MaybeSend + MaybeSync {
    type ResolvedEnvelope: Envelope<'static> + MaybeSend + MaybeSync;
    /// Resolve dependencies, starting with a list of dependencies. Should try to resolve
    /// all dependents after `dependency`, if `Dependency` is missing as well.
    /// * Once resolved, these dependencies may have missing dependencies of their own.
    /// # Returns
    /// * `Vec<Self::ResolvedEnvelope>`: The list of envelopes which were resolved.
    async fn resolve(
        &self,
        missing: HashSet<MissingEnvelope>,
    ) -> Result<Resolved<Self::ResolvedEnvelope>, ResolutionError>;
}

#[xmtp_common::async_trait]
impl<T> ResolveDependencies for &T
where
    T: ResolveDependencies,
{
    type ResolvedEnvelope = T::ResolvedEnvelope;
    async fn resolve(
        &self,
        missing: HashSet<MissingEnvelope>,
    ) -> Result<Resolved<Self::ResolvedEnvelope>, ResolutionError> {
        <T as ResolveDependencies>::resolve(*self, missing).await
    }
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[xmtp_common::async_trait]
impl ResolveDependencies for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(&self, m: HashSet<MissingEnvelope>) -> Result<Resolved<()>, ResolutionError> {
        Ok(Resolved {
            resolved: vec![],
            unresolved: Some(m),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ResolutionError {
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
    #[error(transparent)]
    Body(#[from] BodyError),
    #[error("{0}")]
    Api(Box<dyn RetryableError>),
    #[error(transparent)]
    Build(#[from] UninitializedFieldError),
    #[error("Resolution failed  to find all missing dependant envelopes")]
    ResolutionFailed,
    #[error(transparent)]
    Store(#[from] CursorStoreError),
}

impl RetryableError for ResolutionError {
    fn is_retryable(&self) -> bool {
        use ResolutionError::*;
        match self {
            Envelope(e) => e.is_retryable(),
            Body(b) => b.is_retryable(),
            Api(a) => a.is_retryable(),
            Build(_) => false,
            ResolutionFailed => false,
            Store(s) => s.is_retryable(),
        }
    }
}

impl ResolutionError {
    pub fn api<E: RetryableError + 'static>(e: E) -> Self {
        ResolutionError::Api(Box::new(e))
    }
}
