use crate::messages::decoded_message::{DecodedMessage, MessageBody};
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_configuration::MESSAGE_REACTION_LIMIT;
use xmtp_db::DbQuery;
use xmtp_db::group_message::{
    ContentType as DbContentType, RelationCounts, RelationQuery, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::ContentTypeId;

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
pub struct EnrichedStoredMessage {
    pub delivery_cursor: Option<xmtp_db::delivery::DeliveryCursor>,
    pub stored: StoredGroupMessage,
    pub decoded: DecodedMessage,
    pub parent_stored: Option<StoredGroupMessage>,
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

// A row adds its id and at most one parent id to the relation queries.
// Leave room under SQLite's bound-value limit for the other binds.
const ENRICHMENT_BATCH: usize = 250;

/// Enrich every stored row. Content that fails to decode stays in the list
/// as an undecodable body; only a database failure is an error.
// implements: CTYPE-008
pub fn enrich_messages_with_stored(
    conn: impl DbQuery,
    group_id: &GroupId,
    messages: Vec<StoredGroupMessage>,
) -> Result<Vec<EnrichedStoredMessage>, EnrichMessageError> {
    let mut enriched = Vec::with_capacity(messages.len());
    let mut messages = messages.into_iter();
    while messages.len() > ENRICHMENT_BATCH {
        let batch = messages.by_ref().take(ENRICHMENT_BATCH).collect();
        enriched.extend(enrich_batch(&conn, group_id, batch)?);
    }
    enriched.extend(enrich_batch(&conn, group_id, messages.collect())?);
    Ok(enriched)
}

fn enrich_batch(
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

            // A marked row is already the placeholder (PROC-060).
            // implements: PROC-060
            if stored_message.deleted_by_inbox_id.is_none() {
                decoded.reactions = relations
                    .reactions
                    .remove(&decoded.metadata.id)
                    .unwrap_or_default();

                decoded.num_replies = relations
                    .reply_counts
                    .get(&decoded.metadata.id)
                    .cloned()
                    .unwrap_or(0);

                decoded.num_reaction_messages = relations
                    .reaction_counts
                    .get(&decoded.metadata.id)
                    .cloned()
                    .unwrap_or(0);

                // Handle Reply messages - populate in_reply_to field. A marked
                // parent decodes as the placeholder.
                if let MessageBody::Reply(mut reply_body) = decoded.content {
                    // The decoder accepted only a hex reference.
                    if let Ok(id) = hex::decode(&reply_body.reference_id)
                        && let Some((stored, decoded)) = relations.referenced_messages.get(&id)
                    {
                        parent_stored = Some(stored.clone());
                        reply_body.in_reply_to = Some(Box::new(decoded.clone()));
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
            reaction_counts: HashMap::new(),
            referenced_messages: HashMap::new(),
            reply_counts: HashMap::new(),
        });
    }

    let of_type = |content_type| RelationQuery {
        content_types: Some(vec![content_type]),
        ..Default::default()
    };
    // Any member can send unlimited reactions, so each message carries only
    // its latest reaction messages and a count of all of them.
    // implements: PROC-053
    let summary_query = RelationQuery {
        limit: Some(i64::from(MESSAGE_REACTION_LIMIT)),
        ..of_type(DbContentType::Reaction)
    };

    let reactions = conn.get_inbound_relations(group_id, message_ids, summary_query)?;
    let reaction_counts =
        conn.get_inbound_relation_counts(group_id, message_ids, of_type(DbContentType::Reaction))?;
    let referenced_messages = conn.get_outbound_relations(group_id, reference_ids)?;
    let reply_counts =
        conn.get_inbound_relation_counts(group_id, message_ids, of_type(DbContentType::Reply))?;

    Ok(GetRelationsResults {
        reactions: get_reactions(reactions),
        reaction_counts,
        referenced_messages: get_referenced_messages(referenced_messages),
        reply_counts,
    })
}

struct GetRelationsResults {
    reactions: ReactionMap,
    reaction_counts: RelationCounts,
    referenced_messages: ReferencedMessageMap,
    reply_counts: RelationCounts,
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
