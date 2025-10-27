use xmtp_proto::types::Cursor;

use crate::protocol::{Envelope, EnvelopeError};

#[allow(async_fn_in_trait)]
pub trait ResolveDependencies<'a> {
    type ResolvedEnvelope: Envelope<'a>;
    /// Resolve dependencies, starting with a list of dependencies. Should try to resolve
    /// all dependents after `dependency`, if `Dependency` is missing as well.
    /// * Once resolved, these dependencies may have missing dependencies of their own.
    /// # Returns
    /// * `Vec<Self::ResolvedEnvelope>`: The list of envelopes which were resolved.
    async fn resolve(
        &mut self,
        missing: Vec<Cursor>,
    ) -> Result<Vec<Self::ResolvedEnvelope>, EnvelopeError>;
}

/// A resolver that does not even attempt to try and get dependencies
pub struct NoopResolver;

#[allow(async_fn_in_trait)]
impl ResolveDependencies<'_> for NoopResolver {
    type ResolvedEnvelope = ();
    async fn resolve(
        &mut self,
        _: Vec<Cursor>,
    ) -> Result<Vec<Self::ResolvedEnvelope>, EnvelopeError> {
        Ok(vec![])
    }
}
