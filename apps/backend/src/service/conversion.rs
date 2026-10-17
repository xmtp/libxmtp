use crate::{
    api,
    db::{StoredEnvelope, StoredMeta},
};
use prost::Message;

impl From<StoredMeta> for api::EnvelopeMeta {
    /// Convert database metadata to the wire metadata shape.
    ///
    /// Stored sequence IDs, hashes, topic bytes, and retention metadata are
    /// copied without recalculation, so retries return the original values.
    fn from(row: StoredMeta) -> Self {
        api::EnvelopeMeta {
            cursor: Some(api::Cursor {
                sequence_id: row.sequence_id as u64,
            }),
            topic: Some(api::Topic { topic: row.topic }),
            server_ns: row.server_ns as u64,
            expiry_ns: row.expiry_ns.unwrap_or_default() as u64,
            message_hash: Some(api::MessageHash {
                hash: Some(api::message_hash::Hash::Sha256(row.message_hash)),
            }),
            is_commit_or_proposal: row.is_commit_or_proposal,
        }
    }
}

impl TryFrom<StoredEnvelope> for api::ServerEnvelope {
    type Error = tonic::Status;
    /// Decode the canonical stored envelope and attach its stored metadata.
    ///
    /// A decode failure indicates a storage invariant violation, not invalid
    /// client input, and is returned as an internal status.
    fn try_from(row: StoredEnvelope) -> Result<Self, Self::Error> {
        let envelope = api::ClientEnvelope::decode(row.payload.as_slice())
            .map_err(|_| tonic::Status::internal("stored envelope is invalid"))?;
        let meta = StoredMeta {
            sequence_id: row.sequence_id,
            topic: row.topic,
            server_ns: row.server_ns,
            expiry_ns: row.expiry_ns,
            message_hash: row.message_hash,
            is_commit_or_proposal: row.is_commit_or_proposal,
        };
        Ok(api::ServerEnvelope {
            meta: Some(meta.into()),
            envelope: Some(envelope),
        })
    }
}
