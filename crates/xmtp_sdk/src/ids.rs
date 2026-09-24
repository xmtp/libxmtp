use crate::XmtpError;
use xmtp_proto::types::GroupId;

/// An XMTP inbox ID.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct InboxID(pub String);

impl TryFrom<String> for InboxID {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(XmtpError::invalid("inbox ID is empty"));
        }
        Ok(Self(value))
    }
}

impl From<InboxID> for String {
    fn from(value: InboxID) -> Self {
        value.0
    }
}

uniffi::custom_type!(InboxID, String);

/// An installation ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct InstallationID(pub String);

impl TryFrom<String> for InstallationID {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 32)?;
        Ok(Self(value))
    }
}

impl From<InstallationID> for String {
    fn from(value: InstallationID) -> Self {
        value.0
    }
}

uniffi::custom_type!(InstallationID, String);

/// A conversation ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ConversationID(pub String);

impl TryFrom<String> for ConversationID {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 16)?;
        Ok(Self(value))
    }
}

impl From<ConversationID> for String {
    fn from(value: ConversationID) -> Self {
        value.0
    }
}

uniffi::custom_type!(ConversationID, String);

impl From<GroupId> for ConversationID {
    fn from(value: GroupId) -> Self {
        Self(hex::encode(value.as_slice()))
    }
}

impl TryFrom<ConversationID> for GroupId {
    type Error = XmtpError;

    fn try_from(value: ConversationID) -> Result<Self, Self::Error> {
        let bytes = hex::decode(value.0).map_err(XmtpError::unknown)?;
        GroupId::try_from(bytes.as_slice()).map_err(XmtpError::unknown)
    }
}

/// A message ID, encoded as lowercase hex.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MessageID(pub String);

impl TryFrom<String> for MessageID {
    type Error = XmtpError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_hex_id(&value, 32)?;
        Ok(Self(value))
    }
}

impl From<MessageID> for String {
    fn from(value: MessageID) -> Self {
        value.0
    }
}

uniffi::custom_type!(MessageID, String);

impl MessageID {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, XmtpError> {
        Self::try_from(hex::encode(bytes))
    }
}

fn validate_hex_id(value: &str, byte_len: usize) -> Result<(), XmtpError> {
    if value.len() != byte_len * 2
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value != value.to_ascii_lowercase()
    {
        return Err(XmtpError::invalid("invalid lowercase hex ID"));
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
