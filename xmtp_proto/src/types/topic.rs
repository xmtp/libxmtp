use smallvec::SmallVec;
use std::{
    fmt::{Debug, Display},
    ops::Deref,
};

use crate::{ConversionError, xmtp::xmtpv4::envelopes::AuthenticatedData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}

impl TryFrom<u8> for TopicKind {
    type Error = crate::ConversionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TopicKind::GroupMessagesV1),
            1 => Ok(TopicKind::WelcomeMessagesV1),
            2 => Ok(TopicKind::IdentityUpdatesV1),
            3 => Ok(TopicKind::KeyPackagesV1),
            i => Err(ConversionError::InvalidValue {
                item: "u8",
                expected: "an unsigned integer in the range 0-3",
                got: i.to_string(),
            }),
        }
    }
}

impl Display for TopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TopicKind::*;
        match self {
            GroupMessagesV1 => write!(f, "group_message_v1"),
            WelcomeMessagesV1 => write!(f, "welcome_message_v1"),
            IdentityUpdatesV1 => write!(f, "identity_updates_v1"),
            KeyPackagesV1 => write!(f, "key_packages_v1"),
        }
    }
}

impl TopicKind {
    fn build<B: AsRef<[u8]>>(&self, bytes: B) -> SmallVec<[u8; 33]> {
        let bytes = bytes.as_ref();
        let mut topic = SmallVec::<[u8; 33]>::new();
        topic.push(*self as u8);
        topic.extend_from_slice(bytes);
        topic
    }

    pub fn create<B: AsRef<[u8]>>(&self, bytes: B) -> Topic {
        Topic {
            inner: self.build(bytes),
        }
    }
}

// inbox id is 32 bytes
// installation id is 32 bytes
// group id is 16 bytes
// so we hold at most 33 bytes at any time
/// A topic where the first byte is the kind
/// https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Topic {
    inner: SmallVec<[u8; 33]>,
}

impl Topic {
    pub fn new(kind: TopicKind, bytes: Vec<u8>) -> Self {
        Self {
            inner: kind.build(bytes),
        }
    }

    pub fn kind(&self) -> TopicKind {
        self.inner[0]
            .try_into()
            .expect("A topic must always be built with a valid `TopicKind`")
    }

    /// Get only the identifying portion of this topic
    pub fn identifier(&self) -> &[u8] {
        &self.inner[1..]
    }

    pub fn bytes(&self) -> &[u8] {
        self.inner.as_slice()
    }

    pub fn to_vec(self) -> Vec<u8> {
        self.inner.to_vec()
    }

    pub fn identity_updates(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::IdentityUpdatesV1 {
            Some(self)
        } else {
            None
        }
    }

    pub fn group_message_v1(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::GroupMessagesV1 {
            Some(self)
        } else {
            None
        }
    }

    pub fn welcome_message_v1(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::WelcomeMessagesV1 {
            Some(self)
        } else {
            None
        }
    }

    pub fn key_packages_v1(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::KeyPackagesV1 {
            Some(self)
        } else {
            None
        }
    }

    /// create a topic from bytes
    /// this is test only. using topics with
    /// invalid byte layout will result in
    /// undefined behavior.
    #[cfg(any(feature = "test-utils", test))]
    pub fn from_bytes_unchecked(bytes: Vec<u8>) -> Self {
        Self {
            inner: SmallVec::from(bytes),
        }
    }
}

impl TryFrom<Vec<u8>> for Topic {
    type Error = ConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if let Some(byte) = value.first() {
            let kind = TopicKind::try_from(*byte)?;
            Ok(Topic::new(kind, value[1..].to_vec()))
        } else {
            Err(ConversionError::InvalidValue {
                item: "Topic",
                expected: "a byte array where the first byte is a valid TopicKind",
                got: hex::encode(value),
            })
        }
    }
}

impl From<Topic> for Vec<u8> {
    fn from(topic: Topic) -> Vec<u8> {
        topic.to_vec()
    }
}

impl Debug for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Topic")
            .field("kind", &self.kind())
            .field("bytes", &hex::encode(self.identifier()))
            .finish()
    }
}

impl Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}/{}]", self.kind(), hex::encode(self.identifier()))
    }
}

impl Deref for Topic {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.as_slice()
    }
}

impl<T> AsRef<T> for Topic
where
    T: ?Sized,
    <Topic as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

impl AsRef<Topic> for Topic {
    fn as_ref(&self) -> &Topic {
        self
    }
}

impl AuthenticatedData {
    pub fn with_topic(topic: Topic) -> AuthenticatedData {
        AuthenticatedData {
            target_topic: topic.to_vec(),
            depends_on: None,
        }
    }
}
