use crate::messages::decoded_message::{DecodedMessage, DeletedBy, MessageBody};
use hex::ToHexExt;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::RetryableError;
use xmtp_db::DbQuery;
use xmtp_db::group_message::{
    ContentType as DbContentType, RelationCounts, RelationQuery, StoredGroupMessage,
};
use xmtp_db::message_deletion::StoredMessageDeletion;

#[derive(Debug, Error)]
pub enum EnrichMessageError {
    #[error("DB error: {0}")]
    DbConnection(#[from] xmtp_db::ConnectionError),
    #[error("Decode error: {0}")]
    CodecError(#[from] xmtp_content_types::CodecError),
    #[error("Decode error: {0}")]
    DecodeError(#[from] prost::DecodeError),
}

impl RetryableError for EnrichMessageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::DbConnection(e) => e.is_retryable(),
            Self::CodecError(_) => false,
            Self::DecodeError(_) => false,
        }
    }
}

// Mapping of reactions, keyed by the ID of the message being reacted to.
type ReactionMap = HashMap<Vec<u8>, Vec<DecodedMessage>>;
// Mapping of referenced messages, keyed by ID
type ReferencedMessageMap = HashMap<Vec<u8>, DecodedMessage>;
// Mapping of deletions, keyed by the ID of the deleted message
type DeletionMap = HashMap<Vec<u8>, StoredMessageDeletion>;

pub fn enrich_messages(
    conn: impl DbQuery,
    group_id: &[u8],
    messages: Vec<StoredGroupMessage>,
) -> Result<Vec<DecodedMessage>, EnrichMessageError> {
    let initial_message_ids: Vec<&[u8]> = messages.iter().map(|m| m.id.as_ref()).collect();

    let reference_ids: Vec<&[u8]> = messages
        .iter()
        .filter_map(|m| m.reference_id.as_deref())
        .collect();

    let mut relations = get_relations(conn, group_id, &initial_message_ids, &reference_ids)?;

    let messages: Vec<DecodedMessage> = messages
        .into_iter()
        .filter_map(|stored_message| {
            let mut decoded = DecodedMessage::try_from(stored_message)
                .inspect_err(|err| tracing::warn!("Failed to decode message {:?}", err))
                .ok()?;

            // Check if this message has been deleted
            if let Some(deletion) = relations.deletions.get(&decoded.metadata.id) {
                // Replace content with DeletedMessage placeholder
                decoded.content = MessageBody::DeletedMessage {
                    deleted_by: if deletion.is_super_admin_deletion {
                        DeletedBy::Admin(deletion.deleted_by_inbox_id.clone())
                    } else {
                        DeletedBy::Sender
                    },
                };
                // Clear reactions and replies for deleted messages
                decoded.reactions = Vec::new();
                decoded.num_replies = 0;
            } else {
                // Only populate reactions and replies for non-deleted messages
                decoded.reactions = relations
                    .reactions
                    .remove(&decoded.metadata.id)
                    .unwrap_or_default();

                decoded.num_replies = relations
                    .reply_counts
                    .get(&decoded.metadata.id)
                    .cloned()
                    .unwrap_or(0);

                if let MessageBody::Reply(mut reply_body) = decoded.content {
                    let _ = hex::decode(&reply_body.reference_id)
                        .inspect_err(|err| {
                            tracing::warn!("could not parse reference ID as hex: {:?}", err)
                        })
                        .inspect(|id| {
                            let in_reply_to = relations.referenced_messages.get(id).cloned();
                            reply_body.in_reply_to = in_reply_to.map(Box::new);
                        });
                    decoded.content = MessageBody::Reply(reply_body);
                }
            }

            Some(decoded)
        })
        .collect();

    Ok(messages)
}

fn get_relations(
    conn: impl DbQuery,
    group_id: &[u8],
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

    // Get deletions for all messages
    let message_ids_vec: Vec<Vec<u8>> = message_ids.iter().map(|id| id.to_vec()).collect();
    let deletions = conn.get_deletions_for_messages(message_ids_vec)?;

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
        .filter_map(|(id, stored_message)| {
            let message_id = id.clone();
            DecodedMessage::try_from(stored_message)
                .inspect_err(|err| {
                    tracing::warn!(
                        "Failed to decode reply root message with ID {} {:?}",
                        message_id.encode_hex(),
                        err
                    );
                })
                .map(|decoded| (id, decoded))
                .ok()
        })
        .collect()
}

fn get_reactions(messages: HashMap<Vec<u8>, Vec<StoredGroupMessage>>) -> ReactionMap {
    messages
        .into_iter()
        .map(|(id, reaction_messages)| {
            let mapped_reactions: Vec<DecodedMessage> = reaction_messages
                .into_iter()
                .filter_map(|stored_msg| {
                    DecodedMessage::try_from(stored_msg)
                        .inspect_err(|err| {
                            tracing::warn!(
                                "Failed to decode message categorized as Reaction: {:?}",
                                err
                            );
                        })
                        .ok()
                })
                .collect();
            (id, mapped_reactions)
        })
        .collect()
}

fn get_deletions(deletions: Vec<StoredMessageDeletion>) -> DeletionMap {
    deletions
        .into_iter()
        .map(|deletion| (deletion.deleted_message_id.clone(), deletion))
        .collect()
}
