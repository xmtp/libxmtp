use std::{collections::HashSet, marker::PhantomData};

use derive_builder::UninitializedFieldError;
use xmtp_common::{MaybeSend, MaybeSync, Retryable, RetryableError};
use xmtp_proto::api::BodyError;

use crate::protocol::{CursorStoreError, Envelope, EnvelopeError, types::RequiredDependency};

pub struct Resolved<E> {
    pub resolved: Vec<E>,
    /// list of envelopes that could not be resolved with this strategy
    pub unresolved: Option<HashSet<RequiredDependency>>,
}

impl<E> Resolved<E> {
    pub fn new(envelopes: Vec<E>, unresolved: Option<HashSet<RequiredDependency>>) -> Self {
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
        missing: HashSet<RequiredDependency>,
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
        missing: HashSet<RequiredDependency>,
    ) -> Result<Resolved<Self::ResolvedEnvelope>, ResolutionError> {
        <T as ResolveDependencies>::resolve(*self, missing).await
    }
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[xmtp_common::async_trait]
impl ResolveDependencies for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(
        &self,
        m: HashSet<RequiredDependency>,
    ) -> Result<Resolved<()>, ResolutionError> {
        Ok(Resolved {
            resolved: vec![],
            unresolved: Some(m),
        })
    }
}

#[derive(Clone, Copy, Default)]
pub struct TypedNoopResolver<T> {
    _marker: PhantomData<T>,
}

impl<T> TypedNoopResolver<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[xmtp_common::async_trait]
impl<T> ResolveDependencies for TypedNoopResolver<T>
where
    T: Envelope<'static>,
{
    type ResolvedEnvelope = T;
    async fn resolve(
        &self,
        m: HashSet<RequiredDependency>,
    ) -> Result<Resolved<T>, ResolutionError> {
        Ok(Resolved {
            resolved: Vec::<T>::new(),
            unresolved: Some(m),
        })
    }
}

#[derive(thiserror::Error, Debug, Retryable)]
pub enum ResolutionError {
    #[error(transparent)]
    #[retry(inherit)]
    Envelope(#[from] EnvelopeError),
    #[error(transparent)]
    #[retry(inherit)]
    Body(#[from] BodyError),
    #[error("{0}")]
    #[retry(inherit)]
    Api(Box<dyn RetryableError>),
    #[error(transparent)]
    Build(#[from] UninitializedFieldError),
    #[error("Resolution failed  to find all missing dependant envelopes")]
    ResolutionFailed,
    #[error(transparent)]
    #[retry(inherit)]
    Store(#[from] CursorStoreError),
}

impl ResolutionError {
    pub fn api<E: RetryableError + 'static>(e: E) -> Self {
        ResolutionError::Api(Box::new(e))
    }
}
