use xmtp_proto::types::TopicCursor;

use crate::protocol::{Envelope, EnvelopeError};

// TODO: sort returns an error because going through the envelopes is
// fallible b/c deserialization.
// https://github.com/xmtp/libxmtp/issues/2691 solves this.

/// Envelopes in a d14n-context must be sorted according to its
/// dependencies, and by-originator.
/// [XIP, cross-originator sorting](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
pub trait Sort<Missing> {
    /// Sort envelopes in-place
    /// elements remaining in `Self` are guaranteed to be sorted.
    /// The sort optionally returns `Missing` elements.
    /// it is up to the caller to resolve any missing envelopes.
    fn sort(self) -> Result<Option<Missing>, EnvelopeError>;
}

/// Extension trait to modify a [`TopicCursor`]
/// with the contents of an envelope.
pub trait ApplyEnvelope<E> {
    /// applies an envelope to a cursor
    fn apply_envelope(&mut self, envelope: &E) -> Result<(), EnvelopeError>;
}

impl<'a, E: Envelope<'a>> ApplyEnvelope<E> for TopicCursor {
    fn apply_envelope(&mut self, envelope: &E) -> Result<(), EnvelopeError> {
        let topic = envelope.topic()?;
        let cursor = envelope.cursor()?;
        self.apply(topic, &cursor);
        Ok(())
    }
}
