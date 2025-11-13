//! Extractor for an MLS Data field
//! useful for verifing a message has been read or maybe duplicates.
use chrono::Utc;
use xmtp_proto::ConversionError;
use xmtp_proto::mls_v1::welcome_message::WelcomePointer;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message::V1 as V3GroupMessage, welcome_message::V1 as V3WelcomeMessage,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

use crate::protocol::{EnvelopeVisitor, Extractor};

/// Extract Mls Data from Envelopes
#[derive(Default, Clone, Debug)]
pub struct TimestampExtractor {
    time: Option<i64>,
}

impl TimestampExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Extractor for TimestampExtractor {
    type Output = Option<chrono::DateTime<Utc>>;

    fn get(self) -> Self::Output {
        self.time.map(chrono::DateTime::from_timestamp_nanos)
    }
}

impl EnvelopeVisitor<'_> for TimestampExtractor {
    type Error = ConversionError;

    fn visit_unsigned_originator(
        &mut self,
        e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.time = Some(e.originator_ns);
        Ok(())
    }

    fn visit_v3_group_message(&mut self, message: &V3GroupMessage) -> Result<(), Self::Error> {
        self.time = Some(message.created_ns as i64);
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, message: &V3WelcomeMessage) -> Result<(), Self::Error> {
        self.time = Some(message.created_ns as i64);
        Ok(())
    }
    fn visit_v3_welcome_pointer(&mut self, ptr: &WelcomePointer) -> Result<(), Self::Error> {
        self.time = Some(ptr.created_ns as i64);
        Ok(())
    }
}
