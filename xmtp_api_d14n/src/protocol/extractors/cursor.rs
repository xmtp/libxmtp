//! Extractor for a envelope [`Cursor`](xmtp_proto::types::Cursor)
//! useful for verifing a message has been read or maybe duplicates.
use xmtp_proto::ConversionError;
use xmtp_proto::mls_v1::welcome_message::WelcomePointer as V3WelcomePointer;
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message::V1 as V3GroupMessage, welcome_message::V1 as V3WelcomeMessage,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

use crate::protocol::{EnvelopeVisitor, Extractor};

/// Extract Cursor from Envelopes
#[derive(Default, Clone, Debug)]
pub struct CursorExtractor {
    cursor: Option<Cursor>,
}

impl CursorExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Extractor for CursorExtractor {
    type Output = Result<Cursor, ConversionError>;

    fn get(self) -> Self::Output {
        self.cursor.ok_or(ConversionError::Missing {
            item: "cursor",
            r#type: std::any::type_name::<Cursor>(),
        })
    }
}

impl EnvelopeVisitor<'_> for CursorExtractor {
    type Error = ConversionError;

    fn visit_unsigned_originator(
        &mut self,
        e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.cursor = Some(Cursor {
            sequence_id: e.originator_sequence_id,
            originator_id: e.originator_node_id,
        });
        Ok(())
    }

    fn visit_v3_group_message(&mut self, m: &V3GroupMessage) -> Result<(), Self::Error> {
        self.cursor = Some(Cursor::v3_messages(m.id));
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, m: &V3WelcomeMessage) -> Result<(), Self::Error> {
        self.cursor = Some(Cursor::v3_welcomes(m.id));
        Ok(())
    }

    fn visit_v3_welcome_pointer(&mut self, m: &V3WelcomePointer) -> Result<(), Self::Error> {
        self.cursor = Some(Cursor::v3_welcomes(m.id));
        Ok(())
    }
}
