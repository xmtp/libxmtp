//! Extractor for an MLS Data field
//! useful for verifying a message has been read or maybe duplicates.
use xmtp_proto::ConversionError;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;

use crate::protocol::{EnvelopeVisitor, Extractor};

/// Extract DependsOn from Envelopes
/// If the envelope does not have dependency, or is already
/// ordered (as is the case for v3), then returns `None`.
#[derive(Default, Clone, Debug)]
pub struct DependsOnExtractor {
    cursor: Option<GlobalCursor>,
}

impl DependsOnExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Extractor for DependsOnExtractor {
    type Output = Option<GlobalCursor>;

    fn get(self) -> Self::Output {
        self.cursor
    }
}

impl EnvelopeVisitor<'_> for DependsOnExtractor {
    type Error = ConversionError;

    fn visit_client(&mut self, e: &ClientEnvelope) -> Result<(), Self::Error> {
        // to avoid clone here & elsewhere
        // https://github.com/xmtp/libxmtp/issues/2691
        self.cursor = e
            .aad
            .as_ref()
            .and_then(|a| a.depends_on.clone().map(Into::into));
        Ok(())
    }
}
