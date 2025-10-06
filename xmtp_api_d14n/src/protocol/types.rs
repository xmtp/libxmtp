#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}

impl TopicKind {
    pub fn build(&self, bytes: &[u8]) -> Vec<u8> {
        let mut topic = Vec::with_capacity(1 + bytes.len());
        topic.push(*self as u8);
        topic.extend_from_slice(bytes);
        topic
    }
}

/// A topic where the first byte is the kind
/// <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes>
pub struct Topic {
    kind: TopicKind,
    bytes: Vec<u8>,
}

impl Topic {
    fn bytes(&self) -> Vec<u8> {
        self.kind.build(&self.bytes)
    }
}

impl From<Topic> for Vec<u8> {
    fn from(topic: Topic) -> Vec<u8> {
        topic.bytes().to_vec()
    }
}
