use std::fmt::{Debug, Display};

use crate::xmtp::xmtpv4::envelopes::AuthenticatedData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}

impl Display for TopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TopicKind::*;
        match self {
            GroupMessagesV1 => write!(f, "{}", "GroupMessages"),
            WelcomeMessagesV1 => write!(f, "{}", "WelcomeMessages"),
            IdentityUpdatesV1 => write!(f, "{}", "IdentityUpdates"),
            KeyPackagesV1 => write!(f, "{}", "KeyPackages"),
        }
    }
}

impl TopicKind {
    pub fn build<B: AsRef<[u8]>>(&self, bytes: B) -> Vec<u8> {
        let bytes = bytes.as_ref();
        let mut topic = Vec::with_capacity(1 + bytes.len());
        topic.push(*self as u8);
        topic.extend_from_slice(bytes);
        topic
    }

    pub fn create<B: AsRef<[u8]>>(&self, bytes: B) -> Topic {
        Topic {
            kind: *self,
            bytes: bytes.as_ref().to_vec(),
        }
    }
}

/// A topic where the first byte is the kind
/// https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes
#[derive(Clone, PartialEq, Eq)]
pub struct Topic {
    pub kind: TopicKind,
    bytes: Vec<u8>,
}

impl Topic {
    pub fn new(kind: TopicKind, bytes: Vec<u8>) -> Self {
        Self { kind, bytes }
    }

    /// Get only the identifying portion of this topic
    pub fn identifier(&self) -> &[u8] {
        &self.bytes
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.kind.build(&self.bytes)
    }

    pub fn to_bytes(self) -> Vec<u8> {
        self.kind.build(self.bytes)
    }
}

impl From<Topic> for Vec<u8> {
    fn from(topic: Topic) -> Vec<u8> {
        topic.to_bytes()
    }
}

impl Debug for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Topic")
            .field("kind", &self.kind)
            .field("bytes", &hex::encode(&self.bytes))
            .finish()
    }
}

impl Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}]", self.kind, hex::encode(&self.bytes))
    }
}

impl AuthenticatedData {
    pub fn with_topic(topic: Topic) -> AuthenticatedData {
        AuthenticatedData {
            target_topic: topic.to_bytes(),
            depends_on: None,
        }
    }
}
