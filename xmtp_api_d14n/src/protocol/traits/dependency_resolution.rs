use std::{collections::HashSet, error::Error};

use derive_builder::UninitializedFieldError;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::api::BodyError;

use crate::protocol::{Envelope, EnvelopeError, types::MissingEnvelope};

#[derive(thiserror::Error, Debug)]
pub enum ResolutionError {
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
    #[error(transparent)]
    Body(#[from] BodyError),
    #[error("{0}")]
    Api(Box<dyn Error>),
    #[error(transparent)]
    Build(#[from] UninitializedFieldError),
    #[error("Resolution failed  to find all missing dependant envelopes")]
    ResolutionFailed,
}

pub struct Resolved<E> {
    pub envelopes: Vec<E>,
    /// list of envelopes that could not be resolved with this strategy
    pub unresolved: Option<HashSet<MissingEnvelope>>,
}

impl<E> Resolved<E> {
    pub fn new(envelopes: Vec<E>, unresolved: Option<HashSet<MissingEnvelope>>) -> Self {
        Self {
            envelopes,
            unresolved,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ResolveDependencies: MaybeSend + MaybeSync {
    type ResolvedEnvelope: Envelope<'static> + MaybeSend + MaybeSync;
    /// Resolve dependencies, starting with a list of dependencies. Should try to resolve
    /// all dependents after `dependency`, if `Dependency` is missing as well.
    /// * Once resolved, these dependencies may have missing dependencies of their own.
    /// # Returns
    /// * `Vec<Self::ResolvedEnvelope>`: The list of envelopes which were resolved.
    async fn resolve(
        &mut self,
        missing: HashSet<MissingEnvelope>,
    ) -> Result<Resolved<Self::ResolvedEnvelope>, ResolutionError>;
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ResolveDependencies for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(
        &mut self,
        m: HashSet<MissingEnvelope>,
    ) -> Result<Resolved<()>, ResolutionError> {
        Ok(Resolved {
            envelopes: vec![],
            unresolved: Some(m),
        })
    }
}
