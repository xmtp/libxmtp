//! turns an envelope back into its protobuf `Body` bytes
use prost::Message;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message::V1 as V3GroupMessage, welcome_message::V1 as V3WelcomeMessage,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;

use crate::protocol::{EnvelopeVisitor, Extractor};

// it should be infalliable that an `Envelope` can be turned back into
// bytes.
/// Extract Mls Data from Envelopes
#[derive(Default, Clone, Debug)]
pub struct BytesExtractor {
    buffer: Vec<u8>,
}

impl BytesExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Extractor for BytesExtractor {
    type Output = Vec<u8>;

    fn get(self) -> Self::Output {
        self.buffer
    }
}

impl EnvelopeVisitor<'_> for BytesExtractor {
    type Error = ConversionError;
    fn visit_originator(&mut self, e: &OriginatorEnvelope) -> Result<(), Self::Error> {
        e.encode(&mut self.buffer)?;
        Ok(())
    }

    fn visit_v3_group_message(&mut self, e: &V3GroupMessage) -> Result<(), Self::Error> {
        e.encode(&mut self.buffer)?;
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, e: &V3WelcomeMessage) -> Result<(), Self::Error> {
        e.encode(&mut self.buffer)?;
        Ok(())
    }
}
