
use xmtp_common::RetryableError;
use xmtp_proto::types::Cursor;

use crate::protocol::ExtractionError;

use super::{EnvelopeError, Extractor};
use crate::protocol::traits::EnvelopeVisitor;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as GroupMessagesFilter;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as WelcomeMessagesFilter;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Extract Cursors from Envelopes
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
    type Output = Result<Cursor, ExtractionError>;

    fn get(self) -> Self::Output {
        self.cursor.ok_or(CursorExtractionError::Failed).map_err(Into::into)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CursorExtractionError {
    #[error("Cursor extraction failed, no cursor available")]
    Failed,
}

impl RetryableError for CursorExtractionError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl From<CursorExtractionError> for EnvelopeError {
    fn from(err: CursorExtractionError) -> EnvelopeError {
        EnvelopeError::Extraction(ExtractionError::Cursor(err))
    }
}

impl EnvelopeVisitor<'_> for CursorExtractor {
    type Error = CursorExtractionError;
    fn visit_subscribe_group_messages_request(
        &mut self,
        _r: &GroupMessagesFilter,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn visit_subscribe_welcome_messages_request(
        &mut self,
        _r: &WelcomeMessagesFilter,
    ) -> Result<(), Self::Error> {
        todo!()
    }

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
}
