use crate::error::AdmissionError;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub(crate) struct StoredMeta {
    pub sequence_id: i64,
    pub topic: Vec<u8>,
    pub server_ns: i64,
    pub expiry_ns: Option<i64>,
    pub message_hash: Vec<u8>,
    pub is_commit_or_proposal: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredEnvelope {
    pub sequence_id: i64,
    pub topic: Vec<u8>,
    pub server_ns: i64,
    pub expiry_ns: Option<i64>,
    pub message_hash: Vec<u8>,
    pub is_commit_or_proposal: bool,
    pub payload: Vec<u8>,
}

pub(crate) struct EnvelopePage {
    pub envelopes: Vec<StoredEnvelope>,
    pub has_more: bool,
}

pub(crate) struct TopicCursor {
    pub topic: Vec<u8>,
    pub cursor: i64,
}

pub(crate) struct History {
    pub head: i64,
    pub payloads: Vec<Vec<u8>>,
}

pub(crate) struct Projection {
    pub added: HashSet<(String, i16)>,
    pub removed: HashSet<(String, i16)>,
}

pub(crate) struct IdentityAdmission {
    pub inbox_id: Vec<u8>,
    pub head: i64,
}

pub(crate) struct PendingEnvelope {
    pub push_eligible: bool,
    pub sender_hmac: Option<Vec<u8>>,
    pub topic: Vec<u8>,
    pub message_hash: [u8; 32],
    pub payload: Vec<u8>,
    pub is_commit_or_proposal: bool,
    pub identity: Option<IdentityAdmission>,
    pub index: usize,
    pub duplicate: Option<StoredMeta>,
    pub validation: Result<Option<Projection>, AdmissionError>,
    pub retention_ns: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i16)]
pub(crate) enum PushChannel {
    Apns = 1,
    Fcm = 2,
    Http = 3,
}

impl TryFrom<i16> for PushChannel {
    type Error = crate::error::Error;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Apns),
            2 => Ok(Self::Fcm),
            3 => Ok(Self::Http),
            _ => Err(crate::error::Error::Invariant("invalid push channel")),
        }
    }
}

pub(crate) struct PushRecipientRecord {
    pub recipient_id: Vec<u8>,
    pub secret_hash: Vec<u8>,
    pub channel: PushChannel,
    pub delivery: String,
    pub signing_key: Option<Vec<u8>>,
    pub renewed_ns: i64,
}

pub(crate) struct PushSubscriptionRecord {
    pub topic: Vec<u8>,
    pub hmac_epoch_base: Option<i64>,
    pub hmac_keys: [Option<Vec<u8>>; 3],
    pub include_commits: bool,
}

pub(crate) struct RecipientStateRecord {
    pub topic_count: i32,
    pub channel: PushChannel,
    pub renewed_ns: i64,
}

pub(crate) struct SubscriptionChanges {
    pub state: RecipientStateRecord,
    pub added: u64,
    pub removed: u64,
}
