//! Validated string IDs.
//!
//! UniFFI lifts a host string into these types without validation. A failed
//! lift loses its typed error on single-threaded WASM, so validation happens
//! in Rust instead: the text is private, and code reads it only through the
//! checked accessors, which return `InvalidArgument` for a malformed value.
//! The SDK operation that receives the ID returns that error.

use crate::XmtpError;
use xmtp_proto::types::GroupId;

macro_rules! validated_id {
    ($(#[$doc:meta])* $name:ident, $validate:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Eq, PartialEq, Hash)]
        pub struct $name(String);

        #[cfg_attr(feature = "pure-only", allow(dead_code))]
        impl $name {
            /// Returns the ID text, or `InvalidArgument` if it is malformed.
            pub fn checked(&self) -> Result<&str, XmtpError> {
                ($validate)(self.0.as_str())?;
                Ok(&self.0)
            }

            /// Returns the owned ID text, or `InvalidArgument` if it is malformed.
            pub fn into_checked(self) -> Result<String, XmtpError> {
                self.checked()?;
                Ok(self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = XmtpError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                ($validate)(value.as_str())?;
                Ok(Self(value))
            }
        }

        uniffi::custom_type!($name, String, {
            lower: |id| id.0,
            try_lift: |value| Ok($name(value)),
        });
    };
}

validated_id!(
    /// An XMTP inbox ID.
    InboxId,
    validate_inbox_id
);
validated_id!(
    /// An installation ID, encoded as lowercase hex.
    InstallationId,
    |value| validate_hex_id(value, 32)
);
validated_id!(
    /// A conversation ID, encoded as lowercase hex: 64 characters for a keyed
    /// conversation, or 32 for a legacy one from an imported archive.
    ConversationId,
    validate_conversation_id
);
validated_id!(
    /// A message ID, encoded as lowercase hex.
    MessageId,
    |value| validate_hex_id(value, 32)
);

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl InboxId {
    /// Wraps text that the SDK produced. It is not validated.
    pub(crate) fn unchecked(value: String) -> Self {
        Self(value)
    }
}

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl InstallationId {
    /// Wraps text that the SDK produced. It is not validated.
    pub(crate) fn unchecked(value: String) -> Self {
        Self(value)
    }
}

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl InstallationId {
    /// Returns the decoded bytes, or `InvalidArgument` if the ID is malformed.
    pub fn to_bytes(&self) -> Result<Vec<u8>, XmtpError> {
        decode_hex_id(self.checked()?)
    }
}

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl ConversationId {
    /// Returns the decoded bytes, or `InvalidArgument` if the ID is malformed.
    pub fn to_bytes(&self) -> Result<Vec<u8>, XmtpError> {
        decode_hex_id(self.checked()?)
    }
}

#[cfg_attr(feature = "pure-only", allow(dead_code))]
impl MessageId {
    /// Returns the decoded bytes, or `InvalidArgument` if the ID is malformed.
    pub fn to_bytes(&self) -> Result<Vec<u8>, XmtpError> {
        decode_hex_id(self.checked()?)
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, XmtpError> {
        Self::try_from(hex::encode(bytes))
    }
}

impl From<GroupId> for ConversationId {
    fn from(value: GroupId) -> Self {
        Self(hex::encode(value.as_slice()))
    }
}

impl TryFrom<ConversationId> for GroupId {
    type Error = XmtpError;

    fn try_from(value: ConversationId) -> Result<Self, Self::Error> {
        GroupId::try_from(value.to_bytes()?.as_slice()).map_err(XmtpError::from_core)
    }
}

fn validate_inbox_id(value: &str) -> Result<(), XmtpError> {
    if value.is_empty() {
        return Err(XmtpError::invalid_argument("inbox ID is empty"));
    }
    Ok(())
}

fn validate_conversation_id(value: &str) -> Result<(), XmtpError> {
    use xmtp_configuration::{GROUP_ID_BYTES, LEGACY_GROUP_ID_BYTES};
    if value.len() == LEGACY_GROUP_ID_BYTES * 2 {
        validate_hex_id(value, LEGACY_GROUP_ID_BYTES)
    } else {
        validate_hex_id(value, GROUP_ID_BYTES)
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

fn decode_hex_id(value: &str) -> Result<Vec<u8>, XmtpError> {
    hex::decode(value).map_err(|_| XmtpError::invalid_argument("invalid lowercase hex ID"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn group_id(len: usize) -> GroupId {
        GroupId::try_from(vec![0xab; len]).expect("16 and 32 bytes are group ids")
    }

    /// A conversation ID is 64 (keyed) or 32 (legacy) lowercase hex
    /// characters, and no other length or case.
    #[xmtp_common::test(unwrap_try = true)]
    fn conversation_id_accepts_keyed_and_legacy_lengths() {
        for len in [32, 64] {
            let text = "ab".repeat(len / 2);
            let id = ConversationId::try_from(text.clone())?;
            assert_eq!(id.checked()?, text);
            assert_eq!(id.to_bytes()?, vec![0xab; len / 2]);
        }
        for text in [
            "a".repeat(31),
            "a".repeat(63),
            "a".repeat(30),
            "a".repeat(48),
            "a".repeat(66),
            "AB".repeat(32),
            "AB".repeat(16),
            "0x".to_owned() + &"ab".repeat(31),
            "zz".repeat(32),
            String::new(),
        ] {
            assert!(
                ConversationId::try_from(text.clone()).is_err(),
                "{text:?} was accepted"
            );
            // A lifted value is checked on first use.
            assert!(ConversationId(text).checked().is_err());
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn conversation_id_round_trips_both_group_id_lengths() {
        for len in [16, 32] {
            let id = group_id(len);
            let conversation = ConversationId::from(id);
            assert_eq!(conversation.checked()?.len(), len * 2);
            assert_eq!(GroupId::try_from(conversation)?, id);
        }
    }
}
