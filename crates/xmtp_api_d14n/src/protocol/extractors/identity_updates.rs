use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;

use crate::protocol::traits::EnvelopeVisitor;
use crate::protocol::{Extractor, PayloadExtractionError};
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Extract Identity Updates from Envelopes
#[derive(Default)]
pub struct IdentityUpdateExtractor {
    originator_node_id: u32,
    originator_sequence_id: u64,
    server_timestamp_ns: u64,
    update: IdentityUpdate,
}

impl Extractor for IdentityUpdateExtractor {
    type Output = (String, IdentityUpdateLog);

    fn get(self) -> Self::Output {
        (
            self.update.inbox_id.clone(),
            IdentityUpdateLog {
                sequence_id: self.originator_sequence_id,
                server_timestamp_ns: self.server_timestamp_ns,
                update: Some(self.update),
            },
        )
    }
}

/// extract an update from a single envelope
impl IdentityUpdateExtractor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EnvelopeVisitor<'_> for IdentityUpdateExtractor {
    type Error = PayloadExtractionError; // mostly is infallible

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.originator_node_id = envelope.originator_node_id;
        self.originator_sequence_id = envelope.originator_sequence_id;
        self.server_timestamp_ns = envelope.originator_ns as u64;
        Ok(())
    }

    fn visit_identity_update(&mut self, u: &IdentityUpdate) -> Result<(), Self::Error> {
        self.update = u.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolEnvelope;
    use crate::protocol::extractors::test_utils::*;

    #[xmtp_common::test]
    fn test_extract_identity_update() {
        let inbox_id = "test_inbox_id".to_string();
        let envelope = TestEnvelopeBuilder::new()
            .with_identity_update_custom(inbox_id.clone())
            .build();
        let mut extractor = IdentityUpdateExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let (extracted_inbox_id, log) = extractor.get();
        assert_eq!(extracted_inbox_id, inbox_id);
        assert_eq!(log.sequence_id, 0);
        assert_eq!(log.server_timestamp_ns, 0);
        assert!(log.update.is_some());
        assert_eq!(log.update.unwrap().inbox_id, inbox_id);
    }
}
