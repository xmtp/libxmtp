use crate::protocol::{Envelope, EnvelopeError};
use chrono::Utc;
use std::sync::LazyLock;
use xmtp_proto::types::{Cursor, GlobalCursor, Topic, TopicKind};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

static TOPIC: LazyLock<Topic> =
    LazyLock::new(|| Topic::new(TopicKind::GroupMessagesV1, vec![0, 1, 2]));

#[derive(Clone, Debug, PartialEq)]
pub struct TestEnvelope {
    pub cursor: Cursor,
    pub depends_on: GlobalCursor,
}

impl TestEnvelope {
    pub fn has_dependency_on(&self, other: &TestEnvelope) -> bool {
        let originator = other.cursor.originator_id;
        let depends_on_sid = self.depends_on.get(&originator);
        depends_on_sid == other.cursor.sequence_id
    }

    pub fn has_dependency_on_any(&self, other: &[TestEnvelope]) -> bool {
        other.iter().any(|e| self.has_dependency_on(e))
    }
}

impl std::fmt::Display for TestEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cursor {} depends on {}", &self.cursor, &self.depends_on)
    }
}

impl Envelope<'_> for TestEnvelope {
    fn topic(&self) -> Result<xmtp_proto::types::Topic, crate::protocol::EnvelopeError> {
        Ok(TOPIC.clone())
    }

    fn payload(&self) -> Result<Payload, crate::protocol::EnvelopeError> {
        unreachable!()
    }

    fn timestamp(&self) -> Option<chrono::DateTime<Utc>> {
        // Create a deterministic timestamp based on originator_id and sequence_id
        // This ensures envelopes can be sorted by timestamp in tests
        let nanos = (self.cursor.originator_id as i64 * 1_000_000_000)
            + (self.cursor.sequence_id as i64 * 1000);
        chrono::DateTime::from_timestamp_nanos(nanos).into()
    }

    fn client_envelope(&self) -> Result<ClientEnvelope, crate::protocol::EnvelopeError> {
        unreachable!()
    }

    fn group_message(
        &self,
    ) -> Result<Option<xmtp_proto::types::GroupMessage>, crate::protocol::EnvelopeError> {
        unreachable!()
    }

    fn welcome_message(
        &self,
    ) -> Result<Option<xmtp_proto::types::WelcomeMessage>, crate::protocol::EnvelopeError> {
        unreachable!()
    }

    fn consume<E>(&self, _extractor: E) -> Result<E::Output, crate::protocol::EnvelopeError>
    where
        Self: Sized,
        for<'a> crate::protocol::EnvelopeError:
            From<<E as crate::protocol::EnvelopeVisitor<'a>>::Error>,
        for<'a> E: crate::protocol::EnvelopeVisitor<'a> + crate::protocol::Extractor,
    {
        unreachable!()
    }

    fn cursor(&self) -> Result<xmtp_proto::types::Cursor, EnvelopeError> {
        Ok(self.cursor)
    }

    fn depends_on(&self) -> Result<Option<xmtp_proto::types::GlobalCursor>, EnvelopeError> {
        Ok(Some(self.depends_on.clone()))
    }

    fn sha256_hash(&self) -> Result<Vec<u8>, EnvelopeError> {
        unreachable!()
    }

    fn bytes(&self) -> Result<Vec<u8>, EnvelopeError> {
        unreachable!()
    }
}
