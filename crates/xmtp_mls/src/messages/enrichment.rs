use crate::messages::decoded_message::{DecodedMessage, DeletedBy, MessageBody};
use prost::Message;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_db::DbQuery;
use xmtp_db::group_message::{
    ContentType as DbContentType, Deletable, RelationCounts, RelationQuery, StoredGroupMessage,
};
use xmtp_db::message_deletion::StoredMessageDeletion;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

use xmtp_proto::types::GroupId;
/// Content type ID for deleted message placeholders shown in enriched message lists
pub fn deleted_message_content_type() -> ContentTypeId {
    ContentTypeId {
        authority_id: "xmtp.org".to_string(),
        type_id: "deletedMessage".to_string(),
        version_major: 1,
        version_minor: 0,
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum EnrichMessageError {
    #[error("Storage error: {0}")]
    #[error_code(inherit)]
    Storage(#[from] xmtp_db::StorageError),
    #[error("DB error: {0}")]
    #[error_code(inherit)]
    DbConnection(#[from] xmtp_db::ConnectionError),
}

impl From<xmtp_db::diesel::result::Error> for EnrichMessageError {
    fn from(error: xmtp_db::diesel::result::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl RetryableError for EnrichMessageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::DbConnection(e) => e.is_retryable(),
            Self::Storage(e) => e.is_retryable(),
        }
    }
}

// Mapping of reactions, keyed by the ID of the message being reacted to.
type ReactionMap = HashMap<Vec<u8>, Vec<DecodedMessage>>;
// Mapping of referenced messages, keyed by ID (stores both stored and decoded)
type ReferencedMessageMap = HashMap<Vec<u8>, (StoredGroupMessage, DecodedMessage)>;
// Mapping of deletions, keyed by the ID of the deleted message
type DeletionMap = HashMap<Vec<u8>, Vec<StoredMessageDeletion>>;

pub struct EnrichedStoredMessage {
    pub delivery_cursor: Option<xmtp_db::delivery::DeliveryCursor>,
    pub stored: StoredGroupMessage,
    pub decoded: DecodedMessage,
    pub parent_stored: Option<StoredGroupMessage>,
}

/// Check the stored wire type before a deletion uses a cached content type.
// implements: CTYPE-018
pub(crate) fn is_deletable_stored_message(message: &StoredGroupMessage) -> bool {
    if !message.kind.is_deletable() {
        return false;
    }
    let Ok(content) = EncodedContent::decode(message.decrypted_message_bytes.as_slice()) else {
        return false;
    };
    let Some(identifier) = content.r#type else {
        return false;
    };
    DbContentType::from_identifier(
        &identifier.authority_id,
        &identifier.type_id,
        identifier.version_major,
    )
    .is_deletable()
}

/// Validates if a deletion should be applied. Checks group membership and authorization.
// implements: PROC-037
pub(crate) fn is_deletion_valid(
    deletion: &StoredMessageDeletion,
    message: &StoredGroupMessage,
    group_id: &GroupId,
) -> bool {
    if deletion.deleted_message_id != message.id {
        return false;
    }

    if deletion.group_id != *group_id || message.group_id != *group_id {
        return false;
    }

    if !is_deletable_stored_message(message) {
        return false;
    }

    let is_sender = deletion.deleted_by_inbox_id == message.sender_inbox_id;
    is_sender || deletion.is_super_admin_deletion
}

#[xmtp_common::mls_span]
pub fn enrich_messages(
    conn: impl DbQuery,
    group_id: &GroupId,
    messages: Vec<StoredGroupMessage>,
) -> Result<Vec<DecodedMessage>, EnrichMessageError> {
    Ok(enrich_messages_with_stored(conn, group_id, messages)?
        .into_iter()
        .map(|message| message.decoded)
        .collect())
}

/// Replace a message with its deletion placeholder. Every trace of the
/// original content goes: body, raw bytes, nested evidence, type, and fallback.
// implements: PROC-037
fn apply_deletion(
    message: &mut DecodedMessage,
    deletion: &StoredMessageDeletion,
    stored: &StoredGroupMessage,
) {
    message.content = MessageBody::DeletedMessage {
        deleted_by: DeletedBy::new(&deletion.deleted_by_inbox_id, &stored.sender_inbox_id),
    };
    message.metadata.content_type = Some(deleted_message_content_type());
    message.fallback_text = None;
    message.reactions = Vec::new();
    message.num_replies = 0;
}

/// Enrich every stored row. Content that fails to decode stays in the list
/// as an undecodable body; only a database failure is an error.
// implements: CTYPE-008
pub fn enrich_messages_with_stored(
    conn: impl DbQuery,
    group_id: &GroupId,
    messages: Vec<StoredGroupMessage>,
) -> Result<Vec<EnrichedStoredMessage>, EnrichMessageError> {
    let initial_message_ids: Vec<&[u8]> = messages.iter().map(|m| m.id.as_ref()).collect();

    let reference_ids: Vec<&[u8]> = messages
        .iter()
        .filter_map(|m| m.reference_id.as_deref())
        .collect();

    let mut relations = get_relations(conn, group_id, &initial_message_ids, &reference_ids)?;

    let messages: Vec<EnrichedStoredMessage> = messages
        .into_iter()
        .map(|stored_message| {
            let mut decoded = DecodedMessage::from(stored_message.clone());
            let mut parent_stored = None;

            let valid_deletion =
                relations
                    .deletions
                    .get(&decoded.metadata.id)
                    .and_then(|deletions| {
                        deletions
                            .iter()
                            .find(|deletion| is_deletion_valid(deletion, &stored_message, group_id))
                    });

            if let Some(deletion) = valid_deletion {
                apply_deletion(&mut decoded, deletion, &stored_message);
            } else {
                decoded.reactions = relations
                    .reactions
                    .remove(&decoded.metadata.id)
                    .unwrap_or_default();

                decoded.num_replies = relations
                    .reply_counts
                    .get(&decoded.metadata.id)
                    .cloned()
                    .unwrap_or(0);

                // Handle Reply messages - populate in_reply_to field
                if let MessageBody::Reply(mut reply_body) = decoded.content {
                    // The decoder accepted only a hex reference.
                    if let Ok(id) = hex::decode(&reply_body.reference_id) {
                        parent_stored = relations
                            .referenced_messages
                            .get(&id)
                            .map(|(stored, _)| stored.clone());
                        let mut in_reply_to = relations
                            .referenced_messages
                            .get(&id)
                            .map(|(_, decoded)| decoded.clone());

                        if let Some(msg) = in_reply_to.as_mut()
                            && let Some(deletions) = relations.deletions.get(&id)
                            && let Some((stored_msg, _)) = relations.referenced_messages.get(&id)
                            && let Some(deletion) = deletions
                                .iter()
                                .find(|deletion| is_deletion_valid(deletion, stored_msg, group_id))
                        {
                            apply_deletion(msg, deletion, stored_msg);
                        }
                        reply_body.in_reply_to = in_reply_to.map(Box::new);
                    }
                    decoded.content = MessageBody::Reply(reply_body);
                }
            }

            EnrichedStoredMessage {
                delivery_cursor: None,
                stored: stored_message,
                decoded,
                parent_stored,
            }
        })
        .collect();

    Ok(messages)
}

fn get_relations(
    conn: impl DbQuery,
    group_id: &GroupId,
    message_ids: &[&[u8]],
    reference_ids: &[&[u8]],
) -> Result<GetRelationsResults, EnrichMessageError> {
    if message_ids.is_empty() {
        return Ok(GetRelationsResults {
            reactions: HashMap::new(),
            referenced_messages: HashMap::new(),
            reply_counts: HashMap::new(),
            deletions: HashMap::new(),
        });
    }

    let reactions_relations_query = RelationQuery::builder()
        .content_types(Some(vec![DbContentType::Reaction]))
        .build()
        .unwrap_or_default();

    let replies_count_query = RelationQuery::builder()
        .content_types(Some(vec![DbContentType::Reply]))
        .build()
        .unwrap_or_default();

    let reactions = conn.get_inbound_relations(group_id, message_ids, reactions_relations_query)?;
    let referenced_messages = conn.get_outbound_relations(group_id, reference_ids)?;
    let reply_counts =
        conn.get_inbound_relation_counts(group_id, message_ids, replies_count_query)?;

    // Get deletions for all messages AND referenced messages in a single batch query.
    // This ensures that if a reply references a deleted message, we can properly show
    // the deletion state in the reply chain.
    let mut all_ids: Vec<Vec<u8>> = message_ids.iter().map(|id| id.to_vec()).collect();
    all_ids.extend(reference_ids.iter().map(|id| id.to_vec()));
    let deletions = conn.get_deletions_for_messages(all_ids)?;

    Ok(GetRelationsResults {
        reactions: get_reactions(reactions),
        referenced_messages: get_referenced_messages(referenced_messages),
        reply_counts,
        deletions: get_deletions(deletions),
    })
}

struct GetRelationsResults {
    reactions: ReactionMap,
    referenced_messages: ReferencedMessageMap,
    reply_counts: RelationCounts,
    deletions: DeletionMap,
}

fn get_referenced_messages(messages: HashMap<Vec<u8>, StoredGroupMessage>) -> ReferencedMessageMap {
    messages
        .into_iter()
        .map(|(id, stored_message)| {
            let decoded = DecodedMessage::from(stored_message.clone());
            (id, (stored_message, decoded))
        })
        .collect()
}

/// A parent's reaction summary lists the reactions that apply to it. A row
/// with a reaction type but no decodable reaction stays in history as its
/// own retained message; it cannot be applied here.
fn get_reactions(messages: HashMap<Vec<u8>, Vec<StoredGroupMessage>>) -> ReactionMap {
    messages
        .into_iter()
        .map(|(id, reaction_messages)| {
            let mapped_reactions: Vec<DecodedMessage> = reaction_messages
                .into_iter()
                .map(DecodedMessage::from)
                .filter(|decoded| matches!(decoded.content, MessageBody::Reaction(_)))
                .collect();
            (id, mapped_reactions)
        })
        .collect()
}

fn get_deletions(deletions: Vec<StoredMessageDeletion>) -> DeletionMap {
    let mut by_message = DeletionMap::new();
    for deletion in deletions {
        by_message
            .entry(deletion.deleted_message_id.clone())
            .or_default()
            .push(deletion);
    }
    for records in by_message.values_mut() {
        records.sort_by(|a, b| a.deleted_at_ns.cmp(&b.deleted_at_ns).then(a.id.cmp(&b.id)));
    }
    by_message
}
