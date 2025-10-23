use xmtp_common::{MaybeSend, MaybeSync};

use crate::protocol::{Envelope, EnvelopeError, types::MissingEnvelope};

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
    ) -> Result<Vec<Self::ResolvedEnvelope>, EnvelopeError>;
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ResolveDependencies for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(&mut self, _: Vec<MissingEnvelope>) -> Result<Vec<()>, EnvelopeError> {
        Ok(vec![])
    }
}
