//! turns an envelope back into its protobuf `Body` bytes
use crate::protocol::{
    BytesExtractor, CursorExtractor, DependsOnExtractor, ExtractionError, Extractor,
    GroupMessageExtractor,
};
use xmtp_proto::types::OrphanedEnvelope;

/// Extract an [`OrphanedEnvelope`] from a envelope
#[derive(Default, Clone, Debug)]
pub struct OrphanExtractor {
    inner: (
        CursorExtractor,
        DependsOnExtractor,
        BytesExtractor,
        GroupMessageExtractor,
    ),
}

delegate_envelope_visitor!(OrphanExtractor);

impl Extractor for OrphanExtractor {
    type Output = Result<OrphanedEnvelope, ExtractionError>;

    fn get(self) -> Self::Output {
        let extracted = self.inner;
        let mut envelope = OrphanedEnvelope::builder();
        envelope.cursor(extracted.0.get()?);
        envelope.depends_on(extracted.1.get().unwrap_or_default());
        envelope.payload(extracted.2.get());
        Ok(envelope.build()?)
    }
}
