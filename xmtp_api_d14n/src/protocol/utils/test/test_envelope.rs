use crate::protocol::{Envelope, EnvelopeError};
use chrono::Utc;
use std::collections::HashSet;
use std::sync::LazyLock;
use xmtp_proto::types::{Cursor, GlobalCursor, OrphanedEnvelope, Topic, TopicKind};
use xmtp_proto::xmtp::xmtpv4;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

static TOPIC: LazyLock<Topic> =
    LazyLock::new(|| Topic::new(TopicKind::GroupMessagesV1, vec![0, 1, 2]));

#[derive(prost::Message, Clone, PartialEq)]
#[prost(skip_debug)]
pub struct TestEnvelope {
    #[prost(uint64, tag = "1")]
    sequence_id: u64,
    #[prost(uint32, tag = "2")]
    originator_id: u32,
    #[prost(message, tag = "3")]
    depends_on: Option<xmtpv4::envelopes::Cursor>,
}

impl From<&OrphanedEnvelope> for TestEnvelope {
    fn from(value: &OrphanedEnvelope) -> Self {
        TestEnvelope {
            sequence_id: value.cursor.sequence_id,
            originator_id: value.cursor.originator_id,
            depends_on: Some(value.depends_on.clone().into()),
        }
    }
}

impl TestEnvelope {
    pub fn new(sequence_id: u64, originator_id: u32, depends_on: GlobalCursor) -> Self {
        Self {
            sequence_id,
            originator_id,
            depends_on: Some(depends_on.into()),
        }
    }

    pub fn cursor(&self) -> Cursor {
        Cursor::new(self.sequence_id, self.originator_id)
    }

    pub fn depends_on(&self) -> GlobalCursor {
        self.depends_on.clone().unwrap().into()
    }
}

impl TestEnvelope {
    pub fn has_dependency_on(&self, other: &TestEnvelope) -> bool {
        let originator = other.cursor().originator_id;
        let depends_on_sid = self.depends_on().get(&originator);
        depends_on_sid == other.cursor().sequence_id
    }

    pub fn has_dependency_on_any(&self, other: &[TestEnvelope]) -> bool {
        other.iter().any(|e| self.has_dependency_on(e))
    }

    // envelope mut only depend on other envelopes in the set
    pub fn only_depends_on(&self, other: &[TestEnvelope]) -> bool {
        let cursors = self.depends_on();
        let valid = other.iter().map(|e| e.cursor()).collect::<HashSet<_>>();
        cursors.cursors().all(|c| valid.contains(&c))
    }
}

impl std::fmt::Display for TestEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.depends_on().is_empty() {
            write!(
                f,
                "cursor {} depends on {}",
                &self.cursor(),
                &self.depends_on()
            )
        } else {
            write!(f, "cursor {} has no dependencies", &self.cursor())
        }
    }
}

impl std::fmt::Debug for TestEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestEnvelope")
            .field("sequence_id", &self.sequence_id)
            .field("originator_id", &self.originator_id)
            .field("depends_on", &self.depends_on())
            .finish()
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
        let nanos = (self.cursor().originator_id as i64 * 1_000_000_000)
            + (self.cursor().sequence_id as i64 * 1000);
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

    fn cursor(&self) -> Result<xmtp_proto::types::Cursor, EnvelopeError> {
        Ok(self.cursor())
    }

    fn depends_on(&self) -> Result<Option<xmtp_proto::types::GlobalCursor>, EnvelopeError> {
        Ok(Some(self.depends_on.clone().unwrap().into()))
    }

    fn sha256_hash(&self) -> Result<Vec<u8>, EnvelopeError> {
        unreachable!()
    }

    fn bytes(&self) -> Result<Vec<u8>, EnvelopeError> {
        unreachable!()
    }

    fn orphan(&self) -> Result<xmtp_proto::types::OrphanedEnvelope, EnvelopeError> {
        use prost::Message;
        let mut buf = Vec::new();
        self.encode(&mut buf).unwrap();
        Ok(xmtp_proto::types::OrphanedEnvelope::builder()
            .cursor(self.cursor())
            .depends_on(self.depends_on())
            .payload(buf)
            .group_id(vec![0, 1, 2])
            .build()?)
    }
}
