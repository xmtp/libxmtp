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
use crate::impl_fetch;
use derive_builder::Builder;
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    dsl::sql as diesel_sql,
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_content_types::{
    attachment, group_updated, leave_request, membership_change, reaction, read_receipt,
    remote_attachment, reply, text, transaction_reference, wallet_send_calls,
};
use xmtp_proto::types::Cursor;

mod convert;
#[cfg(test)]
pub mod messages_newer_than_tests;
#[cfg(test)]
pub mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Identifiable, Eq, PartialEq)]
#[diesel(table_name = group_messages)]
#[diesel(primary_key(id))]
#[diesel(check_for_backend(Sqlite))]
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
    /// The Originator Node ID
    pub originator_id: i64,
    /// The Message SequenceId
    pub sequence_id: i64,
    /// Time in nanoseconds the message was inserted into the database
    /// This field is automatically set by the database
    pub inserted_at_ns: i64,
    /// Timestamp (in NS) after which the message must be deleted
    pub expire_at_ns: Option<i64>,
}

impl StoredGroupMessage {
    pub fn cursor(&self) -> Cursor {
        Cursor::new(self.sequence_id as u64, self.originator_id as u32)
    }
}

// Separate Insertable struct that excludes inserted_at_ns to let the database set it
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = group_messages)]
struct NewStoredGroupMessage {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub decrypted_message_bytes: Vec<u8>,
    pub sent_at_ns: i64,
    pub kind: GroupMessageKind,
    pub sender_installation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub delivery_status: DeliveryStatus,
    pub content_type: ContentType,
    pub version_major: i32,
    pub version_minor: i32,
    pub authority_id: String,
    pub reference_id: Option<Vec<u8>>,
    pub originator_id: i64,
    pub sequence_id: i64,
    // inserted_at_ns is NOT included - let database set it
    pub expire_at_ns: Option<i64>,
}

impl From<&StoredGroupMessage> for NewStoredGroupMessage {
    fn from(msg: &StoredGroupMessage) -> Self {
        Self {
            id: msg.id.clone(),
            group_id: msg.group_id.clone(),
            decrypted_message_bytes: msg.decrypted_message_bytes.clone(),
            sent_at_ns: msg.sent_at_ns,
            kind: msg.kind,
            sender_installation_id: msg.sender_installation_id.clone(),
            sender_inbox_id: msg.sender_inbox_id.clone(),
            delivery_status: msg.delivery_status,
            content_type: msg.content_type,
            version_major: msg.version_major,
            version_minor: msg.version_minor,
            authority_id: msg.authority_id.clone(),
            reference_id: msg.reference_id.clone(),
            originator_id: msg.originator_id,
            sequence_id: msg.sequence_id,
            expire_at_ns: msg.expire_at_ns,
        }
    }
}

pub struct StoredGroupMessageWithReactions {
    pub message: StoredGroupMessage,
    // Messages who's reference_id matches this message's id
    pub reactions: Vec<StoredGroupMessage>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SortBy {
    #[default]
    SentAt,
    InsertedAt,
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
    WalletSendCalls = 10,
    LeaveRequest = 11,
}

impl ContentType {
    pub fn all() -> Vec<ContentType> {
        vec![
            ContentType::Unknown,
            ContentType::Text,
            ContentType::GroupMembershipChange,
            ContentType::GroupUpdated,
            ContentType::Reaction,
            ContentType::ReadReceipt,
            ContentType::Reply,
            ContentType::Attachment,
            ContentType::RemoteAttachment,
            ContentType::TransactionReference,
            ContentType::WalletSendCalls,
            ContentType::LeaveRequest,
        ]
    }
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
            Self::WalletSendCalls => wallet_send_calls::WalletSendCallsCodec::TYPE_ID,
            Self::LeaveRequest => leave_request::LeaveRequestCodec::TYPE_ID,
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
            wallet_send_calls::WalletSendCallsCodec::TYPE_ID => Self::WalletSendCalls,
            leave_request::LeaveRequestCodec::TYPE_ID => Self::LeaveRequest,
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
            10 => Ok(ContentType::WalletSendCalls),
            11 => Ok(ContentType::LeaveRequest),
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

// Custom store implementation that uses NewStoredGroupMessage to exclude inserted_at_ns
impl<C> crate::Store<C> for StoredGroupMessage
where
    C: crate::ConnectionExt,
{
    type Output = ();
    fn store(&self, into: &C) -> Result<(), crate::StorageError> {
        let new_msg = NewStoredGroupMessage::from(self);
        into.raw_query_write::<_, _>(|conn| {
            diesel::insert_into(group_messages::table)
                .values(&new_msg)
                .execute(conn)
                .map(|_| ())
        })
        .map_err(Into::into)
    }
}

// Custom store_or_ignore implementation that uses NewStoredGroupMessage
impl<C> crate::StoreOrIgnore<C> for StoredGroupMessage
where
    C: crate::ConnectionExt,
{
    type Output = ();

    fn store_or_ignore(&self, into: &C) -> Result<(), crate::StorageError> {
        let new_msg = NewStoredGroupMessage::from(self);
        into.raw_query_write(|conn| {
            diesel::insert_or_ignore_into(group_messages::table)
                .values(&new_msg)
                .execute(conn)
                .map(|_| ())
        })
        .map_err(Into::into)
    }
}

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
    #[builder(default = None)]
    pub exclude_content_types: Option<Vec<ContentType>>,
    #[builder(default = None)]
    pub exclude_sender_inbox_ids: Option<Vec<String>>,
    #[builder(default = None)]
    pub sort_by: Option<SortBy>,
    #[builder(default = None)]
    pub inserted_after_ns: Option<i64>,
    #[builder(default = None)]
    pub inserted_before_ns: Option<i64>,
    #[builder(default = false)]
    pub exclude_disappearing: bool,
}

impl MsgQueryArgs {
    pub fn builder() -> MsgQueryArgsBuilder {
        MsgQueryArgsBuilder::default()
    }
}

#[derive(Default, Clone, Builder)]
pub struct RelationQuery {
    #[builder(default = None)]
    pub content_types: Option<Vec<ContentType>>,
    #[builder(default = None)]
    pub limit: Option<i64>,
    #[builder(default = SortDirection::Ascending)]
    pub direction: SortDirection,
}

impl RelationQuery {
    pub fn builder() -> RelationQueryBuilder {
        RelationQueryBuilder::default()
    }
}

pub type InboundRelations = HashMap<Vec<u8>, Vec<StoredGroupMessage>>;
pub type OutboundRelations = HashMap<Vec<u8>, StoredGroupMessage>;
pub type RelationCounts = HashMap<Vec<u8>, usize>;

pub struct MessagesWithRelations {
    pub messages: Vec<StoredGroupMessage>,
    /// Messages referenced by any item in the `messages` vector, keyed by their ID
    pub outbound_relations: HashMap<Vec<u8>, StoredGroupMessage>,
    /// Messages that reference any item in the `messages` vector, grouped by the reference_id
    pub inbound_relations: HashMap<Vec<u8>, Vec<StoredGroupMessage>>,
}

pub type LatestMessageTimeBySender = HashMap<String, i64>;

pub trait QueryGroupMessage {
    /// Query for group messages
    fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError>;

    /// Count group messages matching the given criteria
    fn count_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<i64, crate::ConnectionError>;

    fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError>;

    /// Query for group messages with their reactions
    fn get_group_messages_with_reactions(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError>;

    fn get_inbound_relations(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<InboundRelations, crate::ConnectionError>;

    fn get_outbound_relations(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
    ) -> Result<OutboundRelations, crate::ConnectionError>;

    fn get_inbound_relation_counts(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<RelationCounts, crate::ConnectionError>;

    /// Get a particular group message
    fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError>;

    fn get_latest_message_times_by_sender<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        allowed_content_types: &[ContentType],
    ) -> Result<LatestMessageTimeBySender, crate::ConnectionError>;

    /// Get a particular group message using the write connection
    fn write_conn_get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError>;

    fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError>;

    fn get_group_message_by_cursor<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        sequence_id: Cursor,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError>;

    fn get_sync_group_messages(
        &self,
        group_id: &[u8],
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError>;

    fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> Result<usize, crate::ConnectionError>;

    fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
    ) -> Result<usize, crate::ConnectionError>;

    fn delete_expired_messages(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;

    fn delete_message_by_id<MessageId: AsRef<[u8]>>(
        &self,
        message_id: MessageId,
    ) -> Result<usize, crate::ConnectionError>;

    fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> Result<Vec<Cursor>, crate::ConnectionError>;

    /// Clear messages from the database with optional filtering.
    ///
    /// # Arguments
    /// * `group_ids` - If provided, only delete messages in these groups. If None, delete from all groups.
    /// * `retention_days` - If provided, only delete messages older than this many days. If None, delete all matching messages.
    ///
    /// # Returns
    /// The number of messages deleted.
    fn clear_messages(
        &self,
        group_ids: Option<&[Vec<u8>]>,
        retention_days: Option<u32>,
    ) -> Result<usize, crate::ConnectionError>;
}

impl<T> QueryGroupMessage for &T
where
    T: QueryGroupMessage,
{
    /// Query for group messages
    fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_messages(group_id, args)
    }

    /// Count group messages matching the given criteria
    fn count_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<i64, crate::ConnectionError> {
        (**self).count_group_messages(group_id, args)
    }

    fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).group_messages_paged(args, offset)
    }

    /// Query for group messages with their reactions
    fn get_group_messages_with_reactions(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError> {
        (**self).get_group_messages_with_reactions(group_id, args)
    }

    fn get_inbound_relations(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<InboundRelations, crate::ConnectionError> {
        (**self).get_inbound_relations(group_id, message_ids, relation_query)
    }

    fn get_outbound_relations(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
    ) -> Result<OutboundRelations, crate::ConnectionError> {
        (**self).get_outbound_relations(group_id, message_ids)
    }

    fn get_inbound_relation_counts(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<RelationCounts, crate::ConnectionError> {
        (**self).get_inbound_relation_counts(group_id, message_ids, relation_query)
    }

    fn get_latest_message_times_by_sender<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        allowed_content_types: &[ContentType],
    ) -> Result<LatestMessageTimeBySender, crate::ConnectionError> {
        (**self).get_latest_message_times_by_sender(group_id, allowed_content_types)
    }

    /// Get a particular group message
    fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_message(id)
    }

    /// Get a particular group message using the write connection
    fn write_conn_get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).write_conn_get_group_message(id)
    }

    fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_message_by_timestamp(group_id, timestamp)
    }

    fn get_group_message_by_cursor<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        cursor: Cursor,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_message_by_cursor(group_id, cursor)
    }

    fn get_sync_group_messages(
        &self,
        group_id: &[u8],
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_sync_group_messages(group_id, offset)
    }

    fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> Result<usize, crate::ConnectionError> {
        (**self).set_delivery_status_to_published(msg_id, timestamp, cursor, message_expire_at_ns)
    }

    fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
    ) -> Result<usize, crate::ConnectionError> {
        (**self).set_delivery_status_to_failed(msg_id)
    }

    fn delete_expired_messages(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).delete_expired_messages()
    }

    fn delete_message_by_id<MessageId: AsRef<[u8]>>(
        &self,
        message_id: MessageId,
    ) -> Result<usize, crate::ConnectionError> {
        (**self).delete_message_by_id(message_id)
    }

    fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> Result<Vec<Cursor>, crate::ConnectionError> {
        (**self).messages_newer_than(cursors_by_group)
    }

    fn clear_messages(
        &self,
        group_ids: Option<&[Vec<u8>]>,
        retention_days: Option<u32>,
    ) -> Result<usize, crate::ConnectionError> {
        (**self).clear_messages(group_ids, retention_days)
    }
}

// Macro to apply common message filters to any boxed query
macro_rules! apply_message_filters {
    ($query:expr, $args:expr) => {{
        let mut query = $query;

        if let Some(sent_after) = $args.sent_after_ns {
            query = query.filter(dsl::sent_at_ns.gt(sent_after));
        }

        if let Some(sent_before) = $args.sent_before_ns {
            query = query.filter(dsl::sent_at_ns.lt(sent_before));
        }

        if let Some(kind) = $args.kind {
            query = query.filter(dsl::kind.eq(kind));
        }

        if let Some(status) = $args.delivery_status {
            query = query.filter(dsl::delivery_status.eq(status));
        }

        if let Some(content_types) = &$args.content_types {
            query = query.filter(dsl::content_type.eq_any(content_types));
        }

        if let Some(exclude_content_types) = &$args.exclude_content_types {
            query = query.filter(dsl::content_type.ne_all(exclude_content_types));
        }

        if let Some(exclude_sender_inbox_ids) = &$args.exclude_sender_inbox_ids {
            query = query.filter(dsl::sender_inbox_id.ne_all(exclude_sender_inbox_ids));
        }

        if let Some(inserted_after_ns) = $args.inserted_after_ns {
            query = query.filter(dsl::inserted_at_ns.gt(inserted_after_ns));
        }

        if let Some(inserted_before_ns) = $args.inserted_before_ns {
            query = query.filter(dsl::inserted_at_ns.lt(inserted_before_ns));
        }

        // Always exclude expired messages (expire_at_ns < now)
        let current_time = now_ns();
        query = query.filter(
            dsl::expire_at_ns
                .is_null()
                .or(dsl::expire_at_ns.gt(current_time)),
        );

        query
    }};
}

impl<C: ConnectionExt> QueryGroupMessage for DbConnection<C> {
    /// Query for group messages
    fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        use crate::schema::{group_messages::dsl, groups::dsl as groups_dsl};

        // Check if this is a DM group
        let is_dm = self.raw_query_read(|conn| {
            groups_dsl::groups
                .filter(groups_dsl::id.eq(group_id))
                .select(groups_dsl::conversation_type)
                .first::<ConversationType>(conn)
        })? == ConversationType::Dm;

        // Start with base query
        let mut query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .into_boxed();

        // Apply common filters using macro
        query = apply_message_filters!(query, args);

        // Apply ordering with a rowid tie-break to ensure indexes get used when sorting.
        query = match (
            args.sort_by.clone().unwrap_or_default(),
            args.direction.clone().unwrap_or_default(),
        ) {
            (SortBy::SentAt, SortDirection::Ascending) => {
                query.order((dsl::sent_at_ns.asc(), diesel_sql::<Integer>("rowid").asc()))
            }
            (SortBy::SentAt, SortDirection::Descending) => query.order((
                dsl::sent_at_ns.desc(),
                diesel_sql::<Integer>("rowid").desc(),
            )),
            (SortBy::InsertedAt, SortDirection::Ascending) => query.order((
                dsl::inserted_at_ns.asc(),
                diesel_sql::<Integer>("rowid").asc(),
            )),
            (SortBy::InsertedAt, SortDirection::Descending) => query.order((
                dsl::inserted_at_ns.desc(),
                diesel_sql::<Integer>("rowid").desc(),
            )),
        };

        if let Some(limit) = args.limit {
            query = query.limit(limit);
        }

        let messages = self.raw_query_read(|conn| query.load::<StoredGroupMessage>(conn))?;

        // Mirroring previous behaviour, if you explicitly want duplicate group updates for DMs
        // you can include that type in the content_types argument.
        let include_duplicate_group_updated = args
            .content_types
            .as_ref()
            .map(|types| types.contains(&ContentType::GroupUpdated))
            .unwrap_or(false);

        let messages = if is_dm && !include_duplicate_group_updated {
            // For DM conversations, do some gymnastics to make sure that there is only one GroupUpdated
            // message and that it is treated as the oldest
            let (group_updated_msgs, non_group_msgs): (Vec<_>, Vec<_>) = messages
                .into_iter()
                .partition(|msg| msg.content_type == ContentType::GroupUpdated);

            let oldest_group_updated = group_updated_msgs
                .into_iter()
                .min_by_key(|msg| msg.sent_at_ns);

            match oldest_group_updated {
                Some(msg) => match args.direction.as_ref().unwrap_or(&SortDirection::Ascending) {
                    SortDirection::Ascending => [vec![msg], non_group_msgs].concat(),
                    SortDirection::Descending => [non_group_msgs, vec![msg]].concat(),
                },
                None => non_group_msgs,
            }
        } else {
            messages
        };

        Ok(messages)
    }

    /// Count group messages matching the given criteria
    fn count_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<i64, crate::ConnectionError> {
        use crate::schema::{group_messages::dsl, groups::dsl as groups_dsl};

        // Check if this is a DM group
        let is_dm = self.raw_query_read(|conn| {
            groups_dsl::groups
                .filter(groups_dsl::id.eq(group_id))
                .select(groups_dsl::conversation_type)
                .first::<ConversationType>(conn)
        })? == ConversationType::Dm;

        let include_group_updated = args
            .content_types
            .as_ref()
            .map(|types| types.contains(&ContentType::GroupUpdated))
            .unwrap_or(false);

        // Start with base query
        let mut query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .into_boxed();

        // For DM groups, exclude GroupUpdated messages unless specifically requested
        // In find_group_messages we do some post-query deduplication to return the first GroupUpdated
        // message but not the subsequent ones. That's not really an option here, so instead we are excluding
        // them altogether.
        //
        // Ideally we would prevent the duplicate GroupUpdated messages from being inserted in the first place.
        if is_dm && !include_group_updated {
            query = query.filter(dsl::content_type.ne(ContentType::GroupUpdated));
        }

        // Apply common filters using macro
        query = apply_message_filters!(query, args);

        let count =
            self.raw_query_read(|conn| query.select(diesel::dsl::count_star()).first::<i64>(conn))?;

        Ok(count)
    }

    fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        let MsgQueryArgs {
            sent_after_ns,
            sent_before_ns,
            limit,
            exclude_disappearing,
            ..
        } = args;

        let mut query = group_messages::table
            .left_join(groups::table)
            .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .order_by(group_messages::id)
            .into_boxed();

        if let Some(start_ns) = sent_after_ns {
            query = query.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = sent_before_ns {
            query = query.filter(group_messages::sent_at_ns.le(end_ns));
        }
        if *exclude_disappearing {
            query = query.filter(group_messages::expire_at_ns.is_null());
        } else {
            // Always exclude expired messages (expire_at_ns < now)
            let current_time = now_ns();
            query = query.filter(
                group_messages::expire_at_ns
                    .is_null()
                    .or(group_messages::expire_at_ns.gt(current_time)),
            );
        }

        query = query.limit(limit.unwrap_or(100)).offset(offset);

        self.raw_query_read(|conn| {
            query
                .select(group_messages::all_columns)
                .load::<StoredGroupMessage>(conn)
        })
    }

    /// Query for group messages with their reactions
    fn get_group_messages_with_reactions(
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
            .filter(group_id_filter(group_id))
            .filter(dsl::reference_id.is_not_null())
            .filter(dsl::reference_id.eq_any(message_ids))
            .into_boxed();

        // Apply the same sorting as the main messages
        reactions_query = match args.direction.as_ref().unwrap_or(&SortDirection::Ascending) {
            SortDirection::Ascending => reactions_query.order(dsl::sent_at_ns.asc()),
            SortDirection::Descending => reactions_query.order(dsl::sent_at_ns.desc()),
        };

        let reactions: Vec<StoredGroupMessage> =
            self.raw_query_read(|conn| reactions_query.load::<StoredGroupMessage>(conn))?;

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

    fn get_inbound_relations(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<InboundRelations, crate::ConnectionError> {
        let mut inbound_relations: HashMap<Vec<u8>, Vec<StoredGroupMessage>> = HashMap::new();

        let mut inbound_relations_query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .filter(dsl::reference_id.is_not_null())
            .filter(dsl::reference_id.eq_any(message_ids))
            .into_boxed();

        if relation_query.direction == SortDirection::Descending {
            inbound_relations_query = inbound_relations_query.order(dsl::sent_at_ns.desc());
        } else {
            inbound_relations_query = inbound_relations_query.order(dsl::sent_at_ns.asc());
        }

        if let Some(content_types) = relation_query.content_types {
            inbound_relations_query =
                inbound_relations_query.filter(dsl::content_type.eq_any(content_types));
        }

        if let Some(limit) = relation_query.limit {
            inbound_relations_query = inbound_relations_query.limit(limit);
        }

        let raw_inbound_relations: Vec<StoredGroupMessage> =
            self.raw_query_read(|conn| inbound_relations_query.load::<StoredGroupMessage>(conn))?;

        for inbound_reference in raw_inbound_relations {
            if let Some(reference_id) = &inbound_reference.reference_id {
                inbound_relations
                    .entry(reference_id.clone())
                    .or_default()
                    .push(inbound_reference);
            }
        }

        Ok(inbound_relations)
    }

    fn get_outbound_relations(
        &self,
        group_id: &[u8],
        reference_ids: &[&[u8]],
    ) -> Result<OutboundRelations, crate::ConnectionError> {
        let outbound_references_query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .filter(dsl::id.eq_any(reference_ids))
            .into_boxed();

        let raw_outbound_references: Vec<StoredGroupMessage> =
            self.raw_query_read(|conn| outbound_references_query.load::<StoredGroupMessage>(conn))?;

        Ok(raw_outbound_references
            .into_iter()
            .map(|outbound| (outbound.id.clone(), outbound))
            .collect())
    }

    fn get_inbound_relation_counts(
        &self,
        group_id: &[u8],
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<RelationCounts, crate::ConnectionError> {
        let mut count_query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .filter(dsl::reference_id.is_not_null())
            .filter(dsl::reference_id.eq_any(message_ids))
            .group_by(dsl::reference_id)
            .select((dsl::reference_id, diesel::dsl::count_star()))
            .into_boxed();

        if let Some(content_types) = relation_query.content_types {
            count_query = count_query.filter(dsl::content_type.eq_any(content_types));
        }

        let raw_counts: Vec<(Option<Vec<u8>>, i64)> =
            self.raw_query_read(|conn| count_query.load(conn))?;

        Ok(raw_counts
            .into_iter()
            .filter_map(|(reference_id, count)| reference_id.map(|id| (id, count as usize)))
            .collect())
    }

    fn get_latest_message_times_by_sender<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        allowed_content_types: &[ContentType],
    ) -> Result<LatestMessageTimeBySender, crate::ConnectionError> {
        let query = dsl::group_messages
            .filter(group_id_filter(group_id.as_ref()))
            .filter(dsl::content_type.eq_any(allowed_content_types))
            .group_by(dsl::sender_inbox_id)
            .select((dsl::sender_inbox_id, diesel::dsl::max(dsl::sent_at_ns)))
            .into_boxed();

        let raw_results: Vec<(String, Option<i64>)> =
            self.raw_query_read(|conn| query.load(conn))?;

        Ok(raw_results
            .into_iter()
            .filter_map(|(sender_inbox_id, max_sent_at_ns)| {
                max_sent_at_ns.map(|sent_at_ns| (sender_inbox_id, sent_at_ns))
            })
            .collect())
    }

    /// Get a particular group message
    fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    /// Get a particular group message using the write connection
    fn write_conn_get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id.as_ref()))
                .filter(dsl::sent_at_ns.eq(&timestamp))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    fn get_group_message_by_cursor<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        cursor: Cursor,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id.as_ref()))
                .filter(dsl::sequence_id.eq(cursor.sequence_id as i64))
                .filter(dsl::originator_id.eq(cursor.originator_id as i64))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    fn get_sync_group_messages(
        &self,
        group_id: &[u8],
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        let query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .order(dsl::sent_at_ns.asc())
            .offset(offset);

        // Using write connection here to avoid potential race-conditions
        self.raw_query_write(|conn| query.load::<StoredGroupMessage>(conn))
    }

    fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> Result<usize, crate::ConnectionError> {
        tracing::info!(
            "Message [{}] published with cursor = {}",
            hex::encode(msg_id),
            cursor
        );
        self.raw_query_write(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((
                    dsl::delivery_status.eq(DeliveryStatus::Published),
                    dsl::sent_at_ns.eq(timestamp as i64),
                    dsl::sequence_id.eq(cursor.sequence_id as i64),
                    dsl::originator_id.eq(cursor.originator_id as i64),
                    dsl::expire_at_ns.eq(message_expire_at_ns),
                ))
                .execute(conn)
        })
    }

    fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
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

    fn delete_expired_messages(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            use diesel::prelude::*;
            let now = now_ns();

            diesel::delete(
                dsl::group_messages
                    .filter(dsl::delivery_status.eq(DeliveryStatus::Published))
                    .filter(dsl::kind.eq(GroupMessageKind::Application))
                    .filter(dsl::expire_at_ns.is_not_null())
                    .filter(dsl::expire_at_ns.le(now)),
            )
            .returning(dsl::id)
            .load::<Vec<u8>>(conn)
        })
    }

    fn delete_message_by_id<MessageId: AsRef<[u8]>>(
        &self,
        message_id: MessageId,
    ) -> Result<usize, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            use diesel::prelude::*;
            diesel::delete(dsl::group_messages.filter(dsl::id.eq(message_id.as_ref())))
                .execute(conn)
        })
    }

    fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> Result<Vec<Cursor>, crate::ConnectionError> {
        use diesel::BoolExpressionMethods;
        use diesel::ExpressionMethods;
        use diesel::prelude::*;

        let mut all_cursors = Vec::new();

        // Convert the HashMap into a Vec for batching
        let groups: Vec<_> = cursors_by_group.iter().collect();

        // Process groups in batches of 100
        for batch in groups.chunks(100) {
            // Build the WHERE clause using Diesel's query builder
            // Start with a false condition that we'll OR with real conditions
            let mut batch_filter = Box::new(dsl::group_id.eq(&[] as &[u8]))
                as Box<
                    dyn BoxableExpression<
                            group_messages::table,
                            Sqlite,
                            SqlType = diesel::sql_types::Bool,
                        >,
                >;

            for (group_id, global_cursor) in batch {
                if global_cursor.is_empty() {
                    // No cursor for this group - include all messages
                    batch_filter = Box::new(batch_filter.or(dsl::group_id.eq(group_id.as_slice())));
                } else {
                    // Build condition for this group: group_id matches AND (originator conditions)
                    let known_originators: Vec<i64> =
                        global_cursor.keys().map(|k| *k as i64).collect();

                    // Start with false condition for originator checks
                    let mut originator_filter = Box::new(dsl::originator_id.eq(-1i64))
                        as Box<
                            dyn BoxableExpression<
                                    group_messages::table,
                                    Sqlite,
                                    SqlType = diesel::sql_types::Bool,
                                >,
                        >;

                    // For each known originator, add: originator_id = X AND sequence_id > Y
                    for (orig_id, seq_id) in global_cursor.iter() {
                        originator_filter = Box::new(
                            originator_filter.or(dsl::originator_id
                                .eq(*orig_id as i64)
                                .and(dsl::sequence_id.gt(*seq_id as i64))),
                        );
                    }

                    // Also include messages from unknown originators
                    originator_filter = Box::new(
                        originator_filter.or(dsl::originator_id.ne_all(known_originators)),
                    );

                    // Combine: this group AND (originator conditions)
                    batch_filter = Box::new(
                        batch_filter
                            .or(dsl::group_id.eq(group_id.as_slice()).and(originator_filter)),
                    );
                }
            }

            // Execute the query
            let messages: Vec<(i64, i64)> = self.raw_query_read(|conn| {
                dsl::group_messages
                    .select((dsl::originator_id, dsl::sequence_id))
                    .filter(batch_filter)
                    .load(conn)
            })?;

            for (originator_id, sequence_id) in messages {
                all_cursors.push(Cursor::new(sequence_id as u64, originator_id as u32));
            }
        }

        Ok(all_cursors)
    }

    fn clear_messages(
        &self,
        group_ids: Option<&[Vec<u8>]>,
        retention_days: Option<u32>,
    ) -> Result<usize, crate::ConnectionError> {
        let mut query = diesel::delete(dsl::group_messages).into_boxed();

        if let Some(group_ids) = group_ids {
            query = query.filter(dsl::group_id.eq_any(group_ids));
        }

        if let Some(days) = retention_days {
            let limit = now_ns().saturating_sub(NS_IN_DAY.saturating_mul(i64::from(days)));
            query = query.filter(dsl::sent_at_ns.lt(limit));
        }

        self.raw_query_write(|conn| query.execute(conn))
    }
}

fn group_id_filter(
    group_id: &[u8],
) -> impl diesel::expression::BoxableExpression<
    group_messages::table,
    diesel::sqlite::Sqlite,
    SqlType = diesel::sql_types::Bool,
> + diesel::expression::NonAggregate {
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
    )
}
