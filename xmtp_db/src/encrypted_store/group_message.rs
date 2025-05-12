use derive_builder::Builder;
use diesel::dsl::sql;
use diesel::sql_types::BigInt;
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Sub;
use xmtp_common::time::now_ns;
use xmtp_content_types::{
    attachment, group_updated, membership_change, reaction, read_receipt, remote_attachment, reply,
    text, transaction_reference,
};

#[cfg(test)]
pub mod tests;

use super::ConnectionExt;
use super::group::ConversationType;
use super::schema::groups;
use super::{
    Sqlite,
    db_connection::DbConnection,
    schema::{
        group_messages::{self, dsl},
        groups::dsl as groups_dsl,
    },
};
use crate::{impl_fetch, impl_store, impl_store_or_ignore};
mod convert;

#[derive(
    Debug, Clone, Serialize, Deserialize, Insertable, Identifiable, Queryable, Eq, PartialEq,
)]
#[diesel(table_name = group_messages)]
#[diesel(primary_key(id))]
/// Successfully processed messages to be returned to the User.
pub struct StoredGroupMessage {
    /// Id of the message.
    pub id: Vec<u8>,
    /// Id of the group this message is tied to.
    pub group_id: Vec<u8>,
    /// Contents of message after decryption.
    pub decrypted_message_bytes: Vec<u8>,
    /// Time in nanoseconds the message was sent.
    pub sent_at_ns: i64,
    /// Group Message Kind Enum: 1 = Application, 2 = MembershipChange
    pub kind: GroupMessageKind,
    /// The ID of the App Installation this message was sent from.
    pub sender_installation_id: Vec<u8>,
    /// The Inbox ID of the Sender
    pub sender_inbox_id: String,
    /// We optimistically store messages before sending.
    pub delivery_status: DeliveryStatus,
    /// The Content Type of the message
    pub content_type: ContentType,
    /// The content type version major
    pub version_major: i32,
    /// The content type version minor
    pub version_minor: i32,
    /// The ID of the authority defining the content type
    pub authority_id: String,
    /// The ID of a referenced message
    pub reference_id: Option<Vec<u8>>,
    /// The Message SequenceId
    pub sequence_id: Option<i64>,
    /// The Originator Node ID
    pub originator_id: Option<i64>,
}

pub struct StoredGroupMessageWithReactions {
    pub message: StoredGroupMessage,
    // Messages who's reference_id matches this message's id
    pub reactions: Vec<StoredGroupMessage>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum GroupMessageKind {
    Application = 1,
    MembershipChange = 2,
}

impl ToSql<Integer, Sqlite> for GroupMessageKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for GroupMessageKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(GroupMessageKind::Application),
            2 => Ok(GroupMessageKind::MembershipChange),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

//Legacy content types found at https://github.com/xmtp/xmtp-js/tree/main/content-types
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, FromSqlRow, AsExpression)]
#[diesel(sql_type = diesel::sql_types::Integer)]
pub enum ContentType {
    Unknown = 0,
    Text = 1,
    GroupMembershipChange = 2,
    GroupUpdated = 3,
    Reaction = 4,
    ReadReceipt = 5,
    Reply = 6,
    Attachment = 7,
    RemoteAttachment = 8,
    TransactionReference = 9,
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let as_string = match self {
            Self::Unknown => "unknown",
            Self::Text => text::TextCodec::TYPE_ID,
            Self::GroupMembershipChange => membership_change::GroupMembershipChangeCodec::TYPE_ID,
            Self::GroupUpdated => group_updated::GroupUpdatedCodec::TYPE_ID,
            Self::Reaction => reaction::ReactionCodec::TYPE_ID,
            Self::ReadReceipt => read_receipt::ReadReceiptCodec::TYPE_ID,
            Self::Attachment => attachment::AttachmentCodec::TYPE_ID,
            Self::RemoteAttachment => remote_attachment::RemoteAttachmentCodec::TYPE_ID,
            Self::Reply => reply::ReplyCodec::TYPE_ID,
            Self::TransactionReference => transaction_reference::TransactionReferenceCodec::TYPE_ID,
        };

        write!(f, "{}", as_string)
    }
}

impl From<String> for ContentType {
    fn from(type_id: String) -> Self {
        match type_id.as_str() {
            text::TextCodec::TYPE_ID => Self::Text,
            membership_change::GroupMembershipChangeCodec::TYPE_ID => Self::GroupMembershipChange,
            group_updated::GroupUpdatedCodec::TYPE_ID => Self::GroupUpdated,
            reaction::ReactionCodec::TYPE_ID => Self::Reaction,
            read_receipt::ReadReceiptCodec::TYPE_ID => Self::ReadReceipt,
            reply::ReplyCodec::TYPE_ID => Self::Reply,
            attachment::AttachmentCodec::TYPE_ID => Self::Attachment,
            remote_attachment::RemoteAttachmentCodec::TYPE_ID => Self::RemoteAttachment,
            transaction_reference::TransactionReferenceCodec::TYPE_ID => Self::TransactionReference,
            _ => Self::Unknown,
        }
    }
}

impl ToSql<Integer, Sqlite> for ContentType
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for ContentType
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(ContentType::Unknown),
            1 => Ok(ContentType::Text),
            2 => Ok(ContentType::GroupMembershipChange),
            3 => Ok(ContentType::GroupUpdated),
            4 => Ok(ContentType::Reaction),
            5 => Ok(ContentType::ReadReceipt),
            6 => Ok(ContentType::Reply),
            7 => Ok(ContentType::Attachment),
            8 => Ok(ContentType::RemoteAttachment),
            9 => Ok(ContentType::TransactionReference),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, FromSqlRow, AsExpression)]
#[diesel(sql_type = Integer)]
pub enum DeliveryStatus {
    Unpublished = 1,
    Published = 2,
    Failed = 3,
}

impl ToSql<Integer, Sqlite> for DeliveryStatus
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for DeliveryStatus
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(DeliveryStatus::Unpublished),
            2 => Ok(DeliveryStatus::Published),
            3 => Ok(DeliveryStatus::Failed),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl_fetch!(StoredGroupMessage, group_messages, Vec<u8>);
impl_store!(StoredGroupMessage, group_messages);
impl_store_or_ignore!(StoredGroupMessage, group_messages);

#[derive(Default, Clone, Builder)]
#[builder(setter(into))]
pub struct MsgQueryArgs {
    #[builder(default = None)]
    pub sent_after_ns: Option<i64>,
    #[builder(default = None)]
    pub sent_before_ns: Option<i64>,
    #[builder(default = None)]
    pub kind: Option<GroupMessageKind>,
    #[builder(default = None)]
    pub delivery_status: Option<DeliveryStatus>,
    #[builder(default = None)]
    pub limit: Option<i64>,
    #[builder(default = None)]
    pub direction: Option<SortDirection>,
    #[builder(default = None)]
    pub content_types: Option<Vec<ContentType>>,
}

impl MsgQueryArgs {
    pub fn builder() -> MsgQueryArgsBuilder {
        MsgQueryArgsBuilder::default()
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    /// Query for group messages
    pub fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        // Get all messages that have a group with an id equal the provided id,
        // or a dm_id equal to the dm_id that belongs to the loaded group with the provided id.
        let mut query = dsl::group_messages
            .filter(
                dsl::group_id.eq_any(
                    groups_dsl::groups
                        .filter(
                            groups_dsl::id.eq(group_id).or(groups_dsl::dm_id.eq_any(
                                groups_dsl::groups
                                    .select(groups_dsl::dm_id)
                                    .filter(groups_dsl::id.eq(group_id))
                                    .into_boxed(),
                            )),
                        )
                        .select(groups_dsl::id),
                ),
            )
            .into_boxed();

        if let Some(sent_after) = args.sent_after_ns {
            query = query.filter(dsl::sent_at_ns.gt(sent_after));
        }

        if let Some(sent_before) = args.sent_before_ns {
            query = query.filter(dsl::sent_at_ns.lt(sent_before));
        }

        if let Some(kind) = args.kind {
            query = query.filter(dsl::kind.eq(kind));
        }

        if let Some(status) = args.delivery_status {
            query = query.filter(dsl::delivery_status.eq(status));
        }

        if let Some(content_types) = &args.content_types {
            query = query.filter(dsl::content_type.eq_any(content_types));
        }

        query = match args.direction.as_ref().unwrap_or(&SortDirection::Ascending) {
            SortDirection::Ascending => query.order(dsl::sent_at_ns.asc()),
            SortDirection::Descending => query.order(dsl::sent_at_ns.desc()),
        };

        if let Some(limit) = args.limit {
            query = query.limit(limit);
        }

        self.raw_query_read(|conn| query.load::<StoredGroupMessage>(conn))
    }

    pub fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        let MsgQueryArgs {
            sent_after_ns,
            sent_before_ns,
            limit,
            ..
        } = args;

        let mut query = group_messages::table
            .left_join(groups::table)
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .select(group_messages::all_columns)
            .order_by(group_messages::id)
            .into_boxed();

        if let Some(start_ns) = sent_after_ns {
            query = query.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = sent_before_ns {
            query = query.filter(group_messages::sent_at_ns.le(end_ns));
        }

        query = query.limit(limit.unwrap_or(100)).offset(offset);
        self.raw_query_read(|conn| query.load::<StoredGroupMessage>(conn))
    }

    /// Query for group messages with their reactions
    pub fn get_group_messages_with_reactions(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError> {
        // First get all the main messages
        let mut modified_args = args.clone();
        // filter out reactions from the main query so we don't get them twice
        let content_types = match modified_args.content_types.clone() {
            Some(content_types) => {
                let mut content_types = content_types.clone();
                content_types.retain(|content_type| *content_type != ContentType::Reaction);
                Some(content_types)
            }
            None => Some(vec![
                ContentType::Text,
                ContentType::GroupMembershipChange,
                ContentType::GroupUpdated,
                ContentType::ReadReceipt,
                ContentType::Reply,
                ContentType::Attachment,
                ContentType::RemoteAttachment,
                ContentType::TransactionReference,
                ContentType::Unknown,
            ]),
        };

        modified_args.content_types = content_types;
        let messages = self.get_group_messages(group_id, &modified_args)?;

        // Then get all reactions for these messages in a single query
        let message_ids: Vec<&[u8]> = messages.iter().map(|m| m.id.as_slice()).collect();

        let mut reactions_query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::reference_id.is_not_null())
            .filter(dsl::reference_id.eq_any(message_ids))
            .into_boxed();

        // Apply the same sorting as the main messages
        reactions_query = match args.direction.as_ref().unwrap_or(&SortDirection::Ascending) {
            SortDirection::Ascending => reactions_query.order(dsl::sent_at_ns.asc()),
            SortDirection::Descending => reactions_query.order(dsl::sent_at_ns.desc()),
        };

        let reactions: Vec<StoredGroupMessage> =
            self.raw_query_read(|conn| reactions_query.load(conn))?;

        // Group reactions by parent message id
        let mut reactions_by_reference: HashMap<Vec<u8>, Vec<StoredGroupMessage>> = HashMap::new();

        for reaction in reactions {
            if let Some(reference_id) = &reaction.reference_id {
                reactions_by_reference
                    .entry(reference_id.clone())
                    .or_default()
                    .push(reaction);
            }
        }

        // Combine messages with their reactions
        let messages_with_reactions: Vec<StoredGroupMessageWithReactions> = messages
            .into_iter()
            .map(|message| {
                let message_clone = message.clone();
                StoredGroupMessageWithReactions {
                    message,
                    reactions: reactions_by_reference
                        .remove(&message_clone.id)
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(messages_with_reactions)
    }

    /// Get a particular group message
    pub fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first(conn)
                .optional()
        })
    }

    /// Get a particular group message using the write connection
    pub fn write_conn_get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first(conn)
                .optional()
        })
    }

    pub fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id.as_ref()))
                .filter(dsl::sent_at_ns.eq(&timestamp))
                .first(conn)
                .optional()
        })
    }

    pub fn get_sync_group_messages(
        &self,
        group_id: &[u8],
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        let query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .order(dsl::sent_at_ns.asc())
            .offset(offset);

        // Using write connection here to avoid potential race-conditions
        self.raw_query_write(|conn| query.load(conn))
    }

    pub fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
    ) -> Result<usize, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((
                    dsl::delivery_status.eq(DeliveryStatus::Published),
                    dsl::sent_at_ns.eq(timestamp as i64),
                ))
                .execute(conn)
        })
    }

    pub fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
    ) -> Result<usize, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((dsl::delivery_status.eq(DeliveryStatus::Failed),))
                .execute(conn)
        })
    }

    pub fn delete_expired_messages(&self) -> Result<usize, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            use diesel::prelude::*;
            let disappear_from_ns = groups_dsl::message_disappear_from_ns
                .assume_not_null()
                .into_sql::<BigInt>();
            let disappear_duration_ns = groups_dsl::message_disappear_in_ns
                .assume_not_null()
                .into_sql::<BigInt>();
            let now = now_ns();

            let expire_messages = dsl::group_messages
                .left_join(
                    groups_dsl::groups.on(sql::<diesel::sql_types::Text>(
                        "lower(hex(group_messages.group_id))",
                    )
                    .eq(sql::<diesel::sql_types::Text>("lower(hex(groups.id))"))),
                )
                .filter(dsl::delivery_status.eq(DeliveryStatus::Published))
                .filter(dsl::kind.eq(GroupMessageKind::Application))
                .filter(
                    groups_dsl::message_disappear_from_ns
                        .is_not_null()
                        .and(groups_dsl::message_disappear_in_ns.is_not_null()),
                )
                .filter(
                    disappear_from_ns
                        .gt(0) // to make sure the settings are correct
                        .and(
                            dsl::sent_at_ns.gt(disappear_from_ns).and(
                                dsl::sent_at_ns.lt(sql::<BigInt>("")
                                    .bind::<BigInt, _>(now)
                                    .assume_not_null()
                                    .sub(disappear_duration_ns)),
                            ),
                        ),
                )
                .select(dsl::id);
            let expired_message_ids = expire_messages.load::<Vec<u8>>(conn)?;

            // Then delete the rows by their IDs
            diesel::delete(dsl::group_messages.filter(dsl::id.eq_any(expired_message_ids)))
                .execute(conn)
        })
    }
}
