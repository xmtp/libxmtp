use crate::messages::decoded_message::{DecodedMessage, DeletedBy, MessageBody};
use hex::ToHexExt;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::RetryableError;
use xmtp_db::DbQuery;
use xmtp_db::group_message::{
    ContentType as DbContentType, Deletable, RelationCounts, RelationQuery, StoredGroupMessage,
};
use xmtp_db::message_deletion::StoredMessageDeletion;
use xmtp_proto::xmtp::mls::message_contents::ContentTypeId;

/// Content type ID for deleted message placeholders shown in enriched message lists
pub fn deleted_message_content_type() -> ContentTypeId {
    ContentTypeId {
        authority_id: "xmtp.org".to_string(),
        type_id: "deletedMessage".to_string(),
        version_major: 1,
        version_minor: 0,
    }
}

#[derive(Debug, Error, xmtp_common::ErrorCode)]
pub enum EnrichMessageError {
    #[error("DB error: {0}")]
    #[error_code(inherit)]
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
// Mapping of referenced messages, keyed by ID (stores both stored and decoded)
type ReferencedMessageMap = HashMap<Vec<u8>, (StoredGroupMessage, DecodedMessage)>;
// Mapping of deletions, keyed by the ID of the deleted message
type DeletionMap = HashMap<Vec<u8>, StoredMessageDeletion>;

/// Validates if a deletion should be applied. Checks group membership and authorization.
pub(crate) fn is_deletion_valid(
    deletion: &StoredMessageDeletion,
    message: &StoredGroupMessage,
    group_id: &[u8],
) -> bool {
    if deletion.deleted_message_id != message.id {
        return false;
    }

    if deletion.group_id != group_id || message.group_id != group_id {
        return false;
    }

    if !message.kind.is_deletable() || !message.content_type.is_deletable() {
        return false;
    }

    let is_sender = deletion.deleted_by_inbox_id == message.sender_inbox_id;
    is_sender || deletion.is_super_admin_deletion
}

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
            let mut decoded = DecodedMessage::try_from(stored_message.clone())
                .inspect_err(|err| tracing::warn!("Failed to decode message {:?}", err))
                .ok()?;

            let valid_deletion = relations
                .deletions
                .get(&decoded.metadata.id)
                .filter(|deletion| is_deletion_valid(deletion, &stored_message, group_id));

            if let Some(deletion) = valid_deletion {
                let is_sender = deletion.deleted_by_inbox_id == stored_message.sender_inbox_id;
                decoded.content = MessageBody::DeletedMessage {
                    deleted_by: if is_sender {
                        DeletedBy::Sender
                    } else {
                        DeletedBy::Admin(deletion.deleted_by_inbox_id.clone())
                    },
                };
                decoded.metadata.content_type = deleted_message_content_type();
                decoded.reactions = Vec::new();
                decoded.num_replies = 0;
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
                    let _ = hex::decode(&reply_body.reference_id)
                        .inspect_err(|err| {
                            tracing::warn!("could not parse reference ID as hex: {:?}", err)
                        })
                        .inspect(|id| {
                            let mut in_reply_to = relations
                                .referenced_messages
                                .get(id)
                                .map(|(_, decoded)| decoded.clone());

                            if let Some(msg) = in_reply_to.as_mut()
                                && let Some(deletion) = relations.deletions.get(id)
                                && let Some((stored_msg, _)) = relations.referenced_messages.get(id)
                                && is_deletion_valid(deletion, stored_msg, group_id)
                            {
                                let is_sender =
                                    deletion.deleted_by_inbox_id == stored_msg.sender_inbox_id;
                                msg.content = MessageBody::DeletedMessage {
                                    deleted_by: if is_sender {
                                        DeletedBy::Sender
                                    } else {
                                        DeletedBy::Admin(deletion.deleted_by_inbox_id.clone())
                                    },
                                };
                                msg.reactions = Vec::new();
                                msg.num_replies = 0;
                            }
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
        .filter_map(|(id, stored_message)| {
            let message_id = id.clone();
            DecodedMessage::try_from(stored_message.clone())
                .inspect_err(|err| {
                    tracing::warn!(
                        "Failed to decode reply root message with ID {} {:?}",
                        message_id.encode_hex(),
                        err
                    );
                })
                .map(|decoded| (id, (stored_message, decoded)))
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
