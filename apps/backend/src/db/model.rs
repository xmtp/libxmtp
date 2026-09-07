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
