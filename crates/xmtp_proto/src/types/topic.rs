use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt::{Debug, Display},
    ops::Deref,
};

use smallvec::SmallVec;

use crate::{ConversionError, types::InstallationId, xmtp::xmtpv4::envelopes::AuthenticatedData};

/// the max size of an item in a [`TopicKind`] is 32 bytes (installation id).
/// the 1st byte is interpreted as the prefixed [`TopicKind`] byte.
type TopicBytes = SmallVec<[u8; 33]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
    CommitLogEntriesV1 = 4,
}

impl TryFrom<u8> for TopicKind {
    type Error = crate::ConversionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TopicKind::GroupMessagesV1),
            1 => Ok(TopicKind::WelcomeMessagesV1),
            2 => Ok(TopicKind::IdentityUpdatesV1),
            3 => Ok(TopicKind::KeyPackagesV1),
            4 => Ok(TopicKind::CommitLogEntriesV1),
            i => Err(ConversionError::InvalidValue {
                item: "u8",
                expected: "an unsigned integer in the range 0-4",
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
            CommitLogEntriesV1 => write!(f, "commit_log_entries_v1"),
        }
    }
}

impl TopicKind {
    fn build<B: AsRef<[u8]>>(&self, bytes: B) -> TopicBytes {
        let bytes = bytes.as_ref();
        let mut topic = TopicBytes::new();
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

/// A topic where the first byte is the kind
/// https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Topic {
    #[serde(serialize_with = "to_hex", deserialize_with = "from_hex")]
    inner: TopicBytes,
}

fn to_hex<S>(bytes: &TopicBytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&hex::encode(bytes.as_slice()))
}

fn from_hex<'de, D>(deserializer: D) -> Result<TopicBytes, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    hex::decode(s)
        .map(SmallVec::from_vec)
        .map_err(serde::de::Error::custom)
}

impl Topic {
    /// Parse a backend topic and check its kind-specific identifier length.
    pub fn parse(bytes: &[u8]) -> Result<Self, ConversionError> {
        use xmtp_configuration::{BACKEND_GROUP_ID_BYTES, BACKEND_INSTALLATION_ID_BYTES};

        let Some((&kind, identifier)) = bytes.split_first() else {
            return Err(ConversionError::InvalidValue {
                item: "Topic",
                expected: "a topic kind followed by its identifier",
                got: "empty".into(),
            });
        };
        let kind = TopicKind::try_from(kind)?;
        let expected = match kind {
            TopicKind::GroupMessagesV1 | TopicKind::CommitLogEntriesV1 => BACKEND_GROUP_ID_BYTES,
            TopicKind::WelcomeMessagesV1
            | TopicKind::IdentityUpdatesV1
            | TopicKind::KeyPackagesV1 => BACKEND_INSTALLATION_ID_BYTES,
        };
        if identifier.len() != expected {
            return Err(ConversionError::InvalidLength {
                item: "Topic identifier",
                expected,
                got: identifier.len(),
            });
        }
        Ok(kind.create(identifier))
    }

    /// Create a commit-log topic from its group identifier.
    pub fn new_commit_log(group_id: impl AsRef<[u8]>) -> Self {
        TopicKind::CommitLogEntriesV1.create(group_id)
    }

    pub fn new(kind: TopicKind, bytes: Vec<u8>) -> Self {
        Self {
            inner: kind.build(bytes),
        }
    }

    /// create a new [`TopicKind::GroupMessagesV1`] topic
    pub fn new_group_message(group_id: impl AsRef<[u8]>) -> Self {
        TopicKind::GroupMessagesV1.create(group_id)
    }

    /// create a new identity update Topic with `inbox_id` bytes
    /// _NOTE_
    /// this function expects the decoded hex from an InboxId,
    /// not the UTF-8 bytes of a InboxId.
    pub fn new_identity_update(inbox_id: impl AsRef<[u8]>) -> Self {
        TopicKind::IdentityUpdatesV1.create(inbox_id)
    }

    /// create a new [`TopicKind::WelcomeMessagesV1`] topic
    /// from an [`InstallationId`]
    pub fn new_welcome_message(installation_id: InstallationId) -> Self {
        TopicKind::WelcomeMessagesV1.create(installation_id)
    }

    /// create a new [`TopicKind::KeyPackagesV1`] topic
    /// from an [`InstallationId`]
    pub fn new_key_package(installation_id: impl AsRef<[u8]>) -> Self {
        TopicKind::KeyPackagesV1.create(installation_id.as_ref())
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

    /// get the full topic bytes as a [`Vec`] by cloning, including the identifying [`TopicKind`]
    pub fn cloned_vec(&self) -> Vec<u8> {
        self.inner.clone().to_vec()
    }

    /// consume this [`Topic`] into its bytes as a Vec
    pub fn to_bytes(self) -> TopicBytes {
        self.inner
    }

    /// treat this topic as a [`TopicKind::IdentityUpdatesV1`],
    /// otherwise returns [`Option::None`].
    /// useful for collection `filter_map` operations when a single topic type
    /// is required
    pub fn identity_updates(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::IdentityUpdatesV1 {
            Some(self)
        } else {
            None
        }
    }

    /// treat this topic as a [`TopicKind::GroupMessagesV1`],
    /// otherwise returns [`Option::None`].
    /// useful for collection `filter_map` operations when a single topic type
    /// is required
    pub fn group_message_v1(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::GroupMessagesV1 {
            Some(self)
        } else {
            None
        }
    }

    /// treat this topic as a [`TopicKind::WelcomeMessagesV1`],
    /// otherwise returns [`Option::None`].
    /// useful for collection `filter_map` operations when a single topic type
    /// is required
    pub fn welcome_message_v1(&self) -> Option<&Topic> {
        if self.kind() == TopicKind::WelcomeMessagesV1 {
            Some(self)
        } else {
            None
        }
    }

    /// treat this topic as a [`TopicKind::KeyPackagesV1`],
    /// otherwise returns [`Option::None`].
    /// useful for collection `filter_map` operations when a single topic type
    /// is required
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
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self {
            inner: SmallVec::from_slice(bytes.as_ref()),
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
        topic.to_bytes().to_vec()
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
        self.inner.deref()
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
            target_topic: topic.into(),
            depends_on: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_configuration::{BACKEND_GROUP_ID_BYTES, BACKEND_INSTALLATION_ID_BYTES};

    /// P1-VAL-01/04: raw backend topics have one known kind and an exact identifier.
    #[xmtp_common::test(unwrap_try = true)]
    fn backend_topic_parser_checks_each_kind_and_identifier_length() {
        assert!(Topic::parse(&[]).is_err());
        assert!(Topic::parse(&[u8::MAX]).is_err());
        for (kind, length) in [
            (TopicKind::GroupMessagesV1, BACKEND_GROUP_ID_BYTES),
            (TopicKind::WelcomeMessagesV1, BACKEND_INSTALLATION_ID_BYTES),
            (TopicKind::IdentityUpdatesV1, BACKEND_INSTALLATION_ID_BYTES),
            (TopicKind::KeyPackagesV1, BACKEND_INSTALLATION_ID_BYTES),
            (TopicKind::CommitLogEntriesV1, BACKEND_GROUP_ID_BYTES),
        ] {
            let mut bytes = vec![kind as u8];
            bytes.extend(vec![0; length]);
            let topic = Topic::parse(&bytes)?;
            assert_eq!(topic.kind(), kind);
            assert_eq!(topic.cloned_vec(), bytes);
            assert!(Topic::parse(&[kind as u8]).is_err());
            bytes.pop();
            assert!(Topic::parse(&bytes).is_err());
            bytes.extend([0, 0]);
            assert!(Topic::parse(&bytes).is_err());
        }
    }
}
