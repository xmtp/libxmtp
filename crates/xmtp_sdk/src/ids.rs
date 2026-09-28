use crate::XmtpError;
use xmtp_proto::types::GroupId;

/// An XMTP inbox ID.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct InboxId(pub String);

impl TryFrom<String> for InboxId {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(XmtpError::invalid_argument("inbox ID is empty"));
        }
        Ok(Self(value))
    }
}

impl From<InboxId> for String {
    fn from(value: InboxId) -> Self {
        value.0
    }
}

uniffi::custom_type!(InboxId, String);

/// An installation ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct InstallationId(pub String);

impl TryFrom<String> for InstallationId {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 32)?;
        Ok(Self(value))
    }
}

impl From<InstallationId> for String {
    fn from(value: InstallationId) -> Self {
        value.0
    }
}

uniffi::custom_type!(InstallationId, String);

/// A conversation ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ConversationId(pub String);

impl TryFrom<String> for ConversationId {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 16)?;
        Ok(Self(value))
    }
}

impl From<ConversationId> for String {
    fn from(value: ConversationId) -> Self {
        value.0
    }
}

uniffi::custom_type!(ConversationId, String);

impl From<GroupId> for ConversationId {
    fn from(value: GroupId) -> Self {
        Self(hex::encode(value.as_slice()))
    }
}

impl TryFrom<ConversationId> for GroupId {
    type Error = XmtpError;

    fn try_from(value: ConversationId) -> Result<Self, Self::Error> {
        let bytes = hex::decode(value.0).map_err(XmtpError::unknown)?;
        GroupId::try_from(bytes.as_slice()).map_err(XmtpError::unknown)
    }
}

/// A message ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MessageId(pub String);

impl TryFrom<String> for MessageId {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 32)?;
        Ok(Self(value))
    }
}

impl From<MessageId> for String {
    fn from(value: MessageId) -> Self {
        value.0
    }
}

uniffi::custom_type!(MessageId, String);

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl MessageId {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, XmtpError> {
        Self::try_from(hex::encode(bytes))
    }
}

fn validate_hex_id(value: &str, byte_len: usize) -> Result<(), XmtpError> {
    if value.len() != byte_len * 2
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value != value.to_ascii_lowercase()
    {
        return Err(XmtpError::invalid_argument("invalid lowercase hex ID"));
    }
    Ok(())
}

/// A timestamp in nanoseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timestamp(pub i64);

impl From<Timestamp> for i64 {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

impl TryFrom<i64> for Timestamp {
    type Error = XmtpError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Self(value))
    }
}

uniffi::custom_type!(Timestamp, i64);
