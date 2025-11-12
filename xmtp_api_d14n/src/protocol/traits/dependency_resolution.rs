use std::error::Error;

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
        missing: Vec<MissingEnvelope>,
    ) -> Result<Vec<Self::ResolvedEnvelope>, ResolutionError>;
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ResolveDependencies for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(&mut self, _: Vec<MissingEnvelope>) -> Result<Vec<()>, ResolutionError> {
        Ok(vec![])
    }
}
