#[cfg(feature = "sqlite")]
use super::ConnectionExt;
use super::group::ConversationType;
#[cfg(feature = "sqlite")]
use super::schema::groups;
#[cfg(feature = "sqlite")]
use super::{
    Sqlite,
    db_connection::DbConnection,
    schema::{
        group_messages::{self, dsl},
        groups::dsl as groups_dsl,
    },
};
#[cfg(feature = "sqlite")]
use crate::impl_fetch;
use derive_builder::Builder;
#[cfg(feature = "sqlite")]
use diesel::{
    deserialize::FromSqlRow, dsl::sql as diesel_sql, expression::AsExpression, prelude::*,
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_content_types::{
    actions, attachment, delete_message, group_updated, intent, leave_request, markdown,
    membership_change, multi_remote_attachment, reaction, read_receipt, remote_attachment, reply,
    text, transaction_reference, wallet_send_calls,
};
use xmtp_proto::types::{Cursor, GroupId};

mod convert;
#[cfg(test)]
pub mod messages_newer_than_tests;
#[cfg(test)]
pub mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, xmtp_macro::PgModel)]
#[xmtp(table = "group_messages")]
#[cfg_attr(feature = "sqlite", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "sqlite", diesel(table_name = group_messages))]
#[cfg_attr(feature = "sqlite", diesel(primary_key(id)))]
#[cfg_attr(feature = "sqlite", diesel(check_for_backend(Sqlite)))]
/// Successfully processed messages to be returned to the User.
pub struct StoredGroupMessage {
    /// Id of the message.
    pub id: Vec<u8>,
    /// Id of the group this message is tied to.
    pub group_id: GroupId,
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
    /// Whether to send a push notification when publishing this message
    pub should_push: bool,
    /// The idempotency key the message id is derived from. Defaults to the send
    /// timestamp, but callers may supply their own to make retries idempotent.
    pub idempotency_key: String,
}

impl StoredGroupMessage {
    pub fn cursor(&self) -> Cursor {
        Cursor::new(self.sequence_id as u64, self.originator_id as u32)
    }
}

// Separate Insertable struct that excludes inserted_at_ns to let the database set it
#[cfg(feature = "sqlite")]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlite", derive(Insertable))]
#[cfg_attr(feature = "sqlite", diesel(table_name = group_messages))]
struct NewStoredGroupMessage {
    pub id: Vec<u8>,
    pub group_id: GroupId,
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
    pub should_push: bool,
    pub idempotency_key: String,
}

#[cfg(feature = "sqlite")]
impl From<&StoredGroupMessage> for NewStoredGroupMessage {
    fn from(msg: &StoredGroupMessage) -> Self {
        Self {
            id: msg.id.clone(),
            group_id: msg.group_id,
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
            should_push: msg.should_push,
            idempotency_key: msg.idempotency_key.clone(),
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
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sqlite", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Integer))]
pub enum GroupMessageKind {
    Application = 1,
    MembershipChange = 2,
}

crate::impl_sql_int_enum!(GroupMessageKind {
    Application = 1,
    MembershipChange = 2,
});

/// Trait for determining if a message can be deleted by users.
pub trait Deletable {
    /// Returns whether this message can be deleted by users.
    fn is_deletable(&self) -> bool;
}

impl Deletable for GroupMessageKind {
    fn is_deletable(&self) -> bool {
        match self {
            // Application messages are deletable
            GroupMessageKind::Application => true,
            // Membership changes are transcript messages - not deletable
            GroupMessageKind::MembershipChange => false,
        }
    }
}

//Legacy content types found at https://github.com/xmtp/xmtp-js/tree/main/content-types
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "sqlite", derive(FromSqlRow, AsExpression))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Integer))]
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
    Markdown = 12,
    Actions = 13,
    Intent = 14,
    MultiRemoteAttachment = 15,
    DeleteMessage = 16,
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
            ContentType::Markdown,
            ContentType::Actions,
            ContentType::Intent,
            ContentType::MultiRemoteAttachment,
            ContentType::DeleteMessage,
        ]
    }
}

impl Deletable for ContentType {
    fn is_deletable(&self) -> bool {
        match self {
            ContentType::GroupMembershipChange
            | ContentType::GroupUpdated
            | ContentType::LeaveRequest
            | ContentType::Reaction
            | ContentType::ReadReceipt
            | ContentType::Actions
            | ContentType::Intent
            | ContentType::DeleteMessage
            // Unknown content types default to non-deletable for safety
            |ContentType::Unknown => false,

            ContentType::Text
            | ContentType::Markdown
            | ContentType::Reply
            | ContentType::Attachment
            | ContentType::RemoteAttachment
            | ContentType::TransactionReference
            | ContentType::MultiRemoteAttachment
            | ContentType::WalletSendCalls => true,
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let as_string = match self {
            Self::Unknown => "unknown",
            Self::Text => text::TextCodec::TYPE_ID,
            Self::Markdown => markdown::MarkdownCodec::TYPE_ID,
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
            Self::Actions => actions::ActionsCodec::TYPE_ID,
            Self::Intent => intent::IntentCodec::TYPE_ID,
            Self::MultiRemoteAttachment => {
                multi_remote_attachment::MultiRemoteAttachmentCodec::TYPE_ID
            }
            Self::DeleteMessage => delete_message::DeleteMessageCodec::TYPE_ID,
        };

        write!(f, "{}", as_string)
    }
}

impl From<String> for ContentType {
    fn from(type_id: String) -> Self {
        match type_id.as_str() {
            text::TextCodec::TYPE_ID => Self::Text,
            markdown::MarkdownCodec::TYPE_ID => Self::Markdown,
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
            actions::ActionsCodec::TYPE_ID => Self::Actions,
            intent::IntentCodec::TYPE_ID => Self::Intent,
            multi_remote_attachment::MultiRemoteAttachmentCodec::TYPE_ID => {
                Self::MultiRemoteAttachment
            }
            delete_message::DeleteMessageCodec::TYPE_ID => Self::DeleteMessage,
            _ => Self::Unknown,
        }
    }
}

crate::impl_sql_int_enum!(ContentType {
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
    Markdown = 12,
    Actions = 13,
    Intent = 14,
    MultiRemoteAttachment = 15,
    DeleteMessage = 16,
});

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sqlite", derive(FromSqlRow, AsExpression))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Integer))]
pub enum DeliveryStatus {
    Unpublished = 1,
    Published = 2,
    Failed = 3,
}

crate::impl_sql_int_enum!(DeliveryStatus {
    Unpublished = 1,
    Published = 2,
    Failed = 3,
});

#[cfg(feature = "sqlite")]
impl_fetch!(StoredGroupMessage, group_messages, Vec<u8>);

// Custom store implementation that uses NewStoredGroupMessage to exclude inserted_at_ns
#[cfg(feature = "sqlite")]
impl<C> crate::Store<C> for StoredGroupMessage
where
    C: crate::ConnectionExt,
{
    type Output = ();
    async fn store(&self, into: &C) -> Result<(), crate::StorageError> {
        let new_msg = NewStoredGroupMessage::from(self);
        into.raw_query::<_, _>(|conn| {
            diesel::insert_into(group_messages::table)
                .values(&new_msg)
                .execute(conn)
                .map(|_| ())
        })
        .map_err(Into::into)
    }
}

// Custom store_or_ignore implementation that uses NewStoredGroupMessage
#[cfg(feature = "sqlite")]
impl<C> crate::StoreOrIgnore<C> for StoredGroupMessage
where
    C: crate::ConnectionExt,
{
    type Output = ();

    async fn store_or_ignore(&self, into: &C) -> Result<(), crate::StorageError> {
        let new_msg = NewStoredGroupMessage::from(self);
        into.raw_query(|conn| {
            diesel::insert_or_ignore_into(group_messages::table)
                .values(&new_msg)
                .execute(conn)
                .map(|_| ())
        })
        .map_err(Into::into)
    }
}

#[derive(Default, Clone, Builder, Debug)]
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
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroupMessage>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Count group messages matching the given criteria
    fn count_group_messages(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> impl std::future::Future<Output = Result<i64, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Return all `Application`-kind messages stored locally for `group_id`
    /// whose `sequence_id` is NOT in the provided list. Used by tools that
    /// compare local state against an authoritative set of sequence ids
    /// (e.g. xdbg's healthcheck validator).
    fn missing_messages(
        &self,
        group_id: &GroupId,
        sequence_ids: &[u64],
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroupMessage>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroupMessage>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Query for group messages with their reactions
    fn get_group_messages_with_reactions(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> impl std::future::Future<
        Output = Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn get_inbound_relations(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> impl std::future::Future<Output = Result<InboundRelations, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn get_outbound_relations(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
    ) -> impl std::future::Future<Output = Result<OutboundRelations, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn get_inbound_relation_counts(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> impl std::future::Future<Output = Result<RelationCounts, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Get a particular group message
    fn get_group_message(
        &self,
        id: &[u8],
    ) -> impl std::future::Future<
        Output = Result<Option<StoredGroupMessage>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn get_latest_message_times_by_sender(
        &self,
        group_id: &GroupId,
        allowed_content_types: &[ContentType],
    ) -> impl std::future::Future<Output = Result<LatestMessageTimeBySender, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Get a particular group message using the write connection
    fn write_conn_get_group_message(
        &self,
        id: &[u8],
    ) -> impl std::future::Future<
        Output = Result<Option<StoredGroupMessage>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn get_group_message_by_timestamp(
        &self,
        group_id: &GroupId,
        timestamp: i64,
    ) -> impl std::future::Future<
        Output = Result<Option<StoredGroupMessage>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn get_group_message_by_cursor(
        &self,
        group_id: &GroupId,
        sequence_id: Cursor,
    ) -> impl std::future::Future<
        Output = Result<Option<StoredGroupMessage>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn set_delivery_status_to_published(
        &self,
        msg_id: &[u8],
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;

    fn set_delivery_status_to_failed(
        &self,
        msg_id: &[u8],
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;

    fn delete_expired_messages(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroupMessage>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// The soonest `expire_at_ns` among published Application messages that have
    /// an expiry set, or `None` if no disappearing messages exist. Note this can
    /// return a timestamp already in the past (an expiry that elapsed while the
    /// worker was asleep) — the caller clamps the resulting sleep to `>= 0` and
    /// deletes on the next wake. Same filters as `delete_expired_messages`
    /// without its `expire_at_ns <= now` bound.
    fn min_expire_at_ns(
        &self,
    ) -> impl std::future::Future<Output = Result<Option<i64>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn delete_message_by_id(
        &self,
        message_id: &[u8],
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Stored messages above each group's cursor, attributed to their group.
    /// The attribution matters: sequence ids are not scoped per group, so a
    /// caller folding these into per-group state must never mix groups.
    fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> impl std::future::Future<Output = Result<Vec<(GroupId, Cursor)>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

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
        group_ids: Option<&[GroupId]>,
        retention_days: Option<u32>,
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;
}

impl<T> QueryGroupMessage for &T
where
    T: QueryGroupMessage + xmtp_common::MaybeSync,
{
    /// Query for group messages
    async fn get_group_messages(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_messages(group_id, args).await
    }

    /// Count group messages matching the given criteria
    async fn count_group_messages(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<i64, crate::ConnectionError> {
        (**self).count_group_messages(group_id, args).await
    }

    async fn missing_messages(
        &self,
        group_id: &GroupId,
        sequence_ids: &[u64],
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).missing_messages(group_id, sequence_ids).await
    }

    async fn group_messages_paged(
        &self,
        args: &MsgQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).group_messages_paged(args, offset).await
    }

    /// Query for group messages with their reactions
    async fn get_group_messages_with_reactions(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError> {
        (**self)
            .get_group_messages_with_reactions(group_id, args)
            .await
    }

    async fn get_inbound_relations(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<InboundRelations, crate::ConnectionError> {
        (**self)
            .get_inbound_relations(group_id, message_ids, relation_query)
            .await
    }

    async fn get_outbound_relations(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
    ) -> Result<OutboundRelations, crate::ConnectionError> {
        (**self).get_outbound_relations(group_id, message_ids).await
    }

    async fn get_inbound_relation_counts(
        &self,
        group_id: &GroupId,
        message_ids: &[&[u8]],
        relation_query: RelationQuery,
    ) -> Result<RelationCounts, crate::ConnectionError> {
        (**self)
            .get_inbound_relation_counts(group_id, message_ids, relation_query)
            .await
    }

    async fn get_latest_message_times_by_sender(
        &self,
        group_id: &GroupId,
        allowed_content_types: &[ContentType],
    ) -> Result<LatestMessageTimeBySender, crate::ConnectionError> {
        (**self)
            .get_latest_message_times_by_sender(group_id, allowed_content_types)
            .await
    }

    /// Get a particular group message
    async fn get_group_message(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_message(id).await
    }

    /// Get a particular group message using the write connection
    async fn write_conn_get_group_message(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).write_conn_get_group_message(id).await
    }

    async fn get_group_message_by_timestamp(
        &self,
        group_id: &GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self)
            .get_group_message_by_timestamp(group_id, timestamp)
            .await
    }

    async fn get_group_message_by_cursor(
        &self,
        group_id: &GroupId,
        cursor: Cursor,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        (**self).get_group_message_by_cursor(group_id, cursor).await
    }

    async fn set_delivery_status_to_published(
        &self,
        msg_id: &[u8],
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> Result<usize, crate::ConnectionError> {
        (**self)
            .set_delivery_status_to_published(msg_id, timestamp, cursor, message_expire_at_ns)
            .await
    }

    async fn set_delivery_status_to_failed(
        &self,
        msg_id: &[u8],
    ) -> Result<usize, crate::ConnectionError> {
        (**self).set_delivery_status_to_failed(msg_id).await
    }

    async fn delete_expired_messages(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        (**self).delete_expired_messages().await
    }

    async fn min_expire_at_ns(&self) -> Result<Option<i64>, crate::ConnectionError> {
        (**self).min_expire_at_ns().await
    }

    async fn delete_message_by_id(
        &self,
        message_id: &[u8],
    ) -> Result<usize, crate::ConnectionError> {
        (**self).delete_message_by_id(message_id).await
    }

    async fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> Result<Vec<(GroupId, Cursor)>, crate::ConnectionError> {
        (**self).messages_newer_than(cursors_by_group).await
    }

    async fn clear_messages(
        &self,
        group_ids: Option<&[GroupId]>,
        retention_days: Option<u32>,
    ) -> Result<usize, crate::ConnectionError> {
        (**self).clear_messages(group_ids, retention_days).await
    }
}

// Macro to apply common message filters to any boxed query.
// Sync track only -- the async impl expresses the same predicates as one
// `$n IS NULL OR ...` block, see `MSG_FILTERS`.
#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> QueryGroupMessage for DbConnection<C> {
    /// Query for group messages
    #[xmtp_common::db_span]
    async fn get_group_messages(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        use crate::schema::group_messages::dsl;

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

        self.raw_query(|conn| query.load::<StoredGroupMessage>(conn))
    }

    /// Count group messages matching the given criteria
    #[xmtp_common::db_span]
    async fn count_group_messages(
        &self,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<i64, crate::ConnectionError> {
        use crate::schema::{group_messages::dsl, groups::dsl as groups_dsl};

        // Check if this is a DM group
        let is_dm = self.raw_query(|conn| {
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
            self.raw_query(|conn| query.select(diesel::dsl::count_star()).first::<i64>(conn))?;

        Ok(count)
    }

    #[xmtp_common::db_span]
    async fn missing_messages(
        &self,
        group_id: &GroupId,
        sequence_ids: &[u64],
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        use crate::schema::group_messages::{self, dsl};
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};

        let sequence_ids: Vec<i64> = sequence_ids.iter().copied().map(|id| id as i64).collect();
        let query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::sequence_id.is_not_null())
            .filter(group_messages::sequence_id.ne_all(sequence_ids))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .order(group_messages::sequence_id.asc());

        self.raw_query(|conn| query.load(conn))
    }

    #[xmtp_common::db_span]
    async fn group_messages_paged(
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

        self.raw_query(|conn| {
            query
                .select(group_messages::all_columns)
                .load::<StoredGroupMessage>(conn)
        })
    }

    /// Query for group messages with their reactions
    #[xmtp_common::db_span]
    async fn get_group_messages_with_reactions(
        &self,
        group_id: &GroupId,
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
        let messages = self.get_group_messages(group_id, &modified_args).await?;

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
            self.raw_query(|conn| reactions_query.load::<StoredGroupMessage>(conn))?;

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

    #[xmtp_common::db_span]
    async fn get_inbound_relations(
        &self,
        group_id: &GroupId,
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
            self.raw_query(|conn| inbound_relations_query.load::<StoredGroupMessage>(conn))?;

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

    #[xmtp_common::db_span]
    async fn get_outbound_relations(
        &self,
        group_id: &GroupId,
        reference_ids: &[&[u8]],
    ) -> Result<OutboundRelations, crate::ConnectionError> {
        let outbound_references_query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .filter(dsl::id.eq_any(reference_ids))
            .into_boxed();

        let raw_outbound_references: Vec<StoredGroupMessage> =
            self.raw_query(|conn| outbound_references_query.load::<StoredGroupMessage>(conn))?;

        Ok(raw_outbound_references
            .into_iter()
            .map(|outbound| (outbound.id.clone(), outbound))
            .collect())
    }

    #[xmtp_common::db_span]
    async fn get_inbound_relation_counts(
        &self,
        group_id: &GroupId,
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
            self.raw_query(|conn| count_query.load(conn))?;

        Ok(raw_counts
            .into_iter()
            .filter_map(|(reference_id, count)| reference_id.map(|id| (id, count as usize)))
            .collect())
    }

    #[xmtp_common::db_span]
    async fn get_latest_message_times_by_sender(
        &self,
        group_id: &GroupId,
        allowed_content_types: &[ContentType],
    ) -> Result<LatestMessageTimeBySender, crate::ConnectionError> {
        let query = dsl::group_messages
            .filter(group_id_filter(group_id))
            .filter(dsl::content_type.eq_any(allowed_content_types))
            .group_by(dsl::sender_inbox_id)
            .select((dsl::sender_inbox_id, diesel::dsl::max(dsl::sent_at_ns)))
            .into_boxed();

        let raw_results: Vec<(String, Option<i64>)> = self.raw_query(|conn| query.load(conn))?;

        Ok(raw_results
            .into_iter()
            .filter_map(|(sender_inbox_id, max_sent_at_ns)| {
                max_sent_at_ns.map(|sent_at_ns| (sender_inbox_id, sent_at_ns))
            })
            .collect())
    }

    /// Get a particular group message
    async fn get_group_message(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    /// Get a particular group message using the write connection
    async fn write_conn_get_group_message(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    async fn get_group_message_by_timestamp(
        &self,
        group_id: &GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id))
                .filter(dsl::sent_at_ns.eq(&timestamp))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    async fn get_group_message_by_cursor(
        &self,
        group_id: &GroupId,
        cursor: Cursor,
    ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id))
                .filter(dsl::sequence_id.eq(cursor.sequence_id as i64))
                .filter(dsl::originator_id.eq(cursor.originator_id as i64))
                .first::<StoredGroupMessage>(conn)
                .optional()
        })
    }

    async fn set_delivery_status_to_published(
        &self,
        msg_id: &[u8],
        timestamp: u64,
        cursor: Cursor,
        message_expire_at_ns: Option<i64>,
    ) -> Result<usize, crate::ConnectionError> {
        tracing::info!(
            "Message [{}] published with cursor = {}",
            hex::encode(msg_id),
            cursor
        );
        self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id))
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

    async fn set_delivery_status_to_failed(
        &self,
        msg_id: &[u8],
    ) -> Result<usize, crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id))
                .set((dsl::delivery_status.eq(DeliveryStatus::Failed),))
                .execute(conn)
        })
    }

    #[xmtp_common::db_span]
    async fn delete_expired_messages(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        self.raw_query(|conn| {
            use diesel::prelude::*;
            let now = now_ns();

            diesel::delete(
                dsl::group_messages
                    .filter(dsl::delivery_status.eq(DeliveryStatus::Published))
                    .filter(dsl::kind.eq(GroupMessageKind::Application))
                    .filter(dsl::expire_at_ns.is_not_null())
                    .filter(dsl::expire_at_ns.le(now)),
            )
            .returning(StoredGroupMessage::as_returning())
            .load::<StoredGroupMessage>(conn)
        })
    }

    #[xmtp_common::db_span]
    async fn min_expire_at_ns(&self) -> Result<Option<i64>, crate::ConnectionError> {
        self.raw_query(|conn| {
            use diesel::dsl::min;
            use diesel::prelude::*;
            dsl::group_messages
                .filter(dsl::delivery_status.eq(DeliveryStatus::Published))
                .filter(dsl::kind.eq(GroupMessageKind::Application))
                .filter(dsl::expire_at_ns.is_not_null())
                .select(min(dsl::expire_at_ns))
                .first::<Option<i64>>(conn)
        })
    }

    async fn delete_message_by_id(
        &self,
        message_id: &[u8],
    ) -> Result<usize, crate::ConnectionError> {
        self.raw_query(|conn| {
            use diesel::prelude::*;
            diesel::delete(dsl::group_messages.filter(dsl::id.eq(message_id))).execute(conn)
        })
    }

    #[xmtp_common::db_span]
    async fn messages_newer_than(
        &self,
        cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
    ) -> Result<Vec<(GroupId, Cursor)>, crate::ConnectionError> {
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
                    batch_filter = Box::new(batch_filter.or(dsl::group_id.eq(group_id)));
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
                        batch_filter.or(dsl::group_id.eq(group_id).and(originator_filter)),
                    );
                }
            }

            // Execute the query
            let messages: Vec<(GroupId, i64, i64)> = self.raw_query(|conn| {
                dsl::group_messages
                    .select((dsl::group_id, dsl::originator_id, dsl::sequence_id))
                    .filter(batch_filter)
                    .load(conn)
            })?;

            for (group_id, originator_id, sequence_id) in messages {
                all_cursors.push((
                    group_id,
                    Cursor::new(sequence_id as u64, originator_id as u32),
                ));
            }
        }

        Ok(all_cursors)
    }

    #[xmtp_common::db_span]
    async fn clear_messages(
        &self,
        group_ids: Option<&[GroupId]>,
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

        self.raw_query(|conn| query.execute(conn))
    }
}

#[cfg(feature = "sqlite")]
fn group_id_filter(
    group_id: &GroupId,
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

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sqlite")`.
#[cfg(all(feature = "sqlx", not(feature = "sqlite"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};
    use sqlx::postgres::PgArguments;
    use sqlx::query::QueryAs;

    /// The `MsgQueryArgs` predicate set, as `$1..$10` in bind order.
    ///
    /// The sync path applies these with the `apply_message_filters!` macro over a
    /// boxed diesel query; here each optional filter is `$n IS NULL OR ...` so
    /// one statement text and one bind order serve every combination. `$10` is
    /// "now", for the always-on expired-message exclusion.
    const MSG_FILTERS: &str = "($1::bigint IS NULL OR sent_at_ns > $1) \
         AND ($2::bigint IS NULL OR sent_at_ns < $2) \
         AND ($3::int4 IS NULL OR kind = $3) \
         AND ($4::int4 IS NULL OR delivery_status = $4) \
         AND ($5::int4[] IS NULL OR content_type = ANY($5)) \
         AND ($6::int4[] IS NULL OR content_type <> ALL($6)) \
         AND ($7::text[] IS NULL OR sender_inbox_id <> ALL($7)) \
         AND ($8::bigint IS NULL OR inserted_at_ns > $8) \
         AND ($9::bigint IS NULL OR inserted_at_ns < $9) \
         AND (expire_at_ns IS NULL OR expire_at_ns > $10)";

    /// The values [`MSG_FILTERS`] binds, owned so they outlive the query.
    ///
    /// Arrays of the `#[repr(i32)]` enums have no `PgHasArrayType`, so they are
    /// converted here rather than at each call site.
    struct MsgFilters {
        sent_after_ns: Option<i64>,
        sent_before_ns: Option<i64>,
        kind: Option<i32>,
        delivery_status: Option<i32>,
        content_types: Option<Vec<i32>>,
        exclude_content_types: Option<Vec<i32>>,
        exclude_sender_inbox_ids: Option<Vec<String>>,
        inserted_after_ns: Option<i64>,
        inserted_before_ns: Option<i64>,
        now_ns: i64,
    }

    fn as_ints(types: &Option<Vec<ContentType>>) -> Option<Vec<i32>> {
        types
            .as_ref()
            .map(|types| types.iter().map(|t| *t as i32).collect())
    }

    impl MsgFilters {
        fn new(args: &MsgQueryArgs) -> Self {
            Self {
                sent_after_ns: args.sent_after_ns,
                sent_before_ns: args.sent_before_ns,
                kind: args.kind.map(|k| k as i32),
                delivery_status: args.delivery_status.map(|s| s as i32),
                content_types: as_ints(&args.content_types),
                exclude_content_types: as_ints(&args.exclude_content_types),
                exclude_sender_inbox_ids: args.exclude_sender_inbox_ids.clone(),
                inserted_after_ns: args.inserted_after_ns,
                inserted_before_ns: args.inserted_before_ns,
                now_ns: now_ns(),
            }
        }

        /// Binds `$1..$10`. The caller's own parameters start at `$11`.
        fn bind<'q, O>(
            &'q self,
            query: QueryAs<'q, sqlx::Postgres, O, PgArguments>,
        ) -> QueryAs<'q, sqlx::Postgres, O, PgArguments> {
            query
                .bind(self.sent_after_ns)
                .bind(self.sent_before_ns)
                .bind(self.kind)
                .bind(self.delivery_status)
                .bind(self.content_types.as_deref())
                .bind(self.exclude_content_types.as_deref())
                .bind(self.exclude_sender_inbox_ids.as_deref())
                .bind(self.inserted_after_ns)
                .bind(self.inserted_before_ns)
                .bind(self.now_ns)
        }
    }

    /// Messages belonging to the group bound at `$n`, or to any group stitched to
    /// it by a shared `dm_id`. Mirrors the sync path's `group_id_filter`: a
    /// non-DM group has a NULL `dm_id`, `dm_id IN (NULL)` matches nothing, and
    /// only the `id = $n` arm applies.
    fn stitched_group_filter(n: usize) -> String {
        format!(
            "group_id IN (SELECT id FROM groups \
             WHERE id = ${n} OR dm_id IN (SELECT dm_id FROM groups WHERE id = ${n}))"
        )
    }

    /// Both sort columns are NOT NULL, so neither ordering needs NULLS LAST. The
    /// `rowid` tie-break is the same one the sync path adds so the index gets
    /// used; Postgres has no implicit rowid, and `migrations_pg` materializes the
    /// column for exactly this.
    fn message_order(args: &MsgQueryArgs) -> &'static str {
        match (
            args.sort_by.clone().unwrap_or_default(),
            args.direction.clone().unwrap_or_default(),
        ) {
            (SortBy::SentAt, SortDirection::Ascending) => "sent_at_ns ASC, rowid ASC",
            (SortBy::SentAt, SortDirection::Descending) => "sent_at_ns DESC, rowid DESC",
            (SortBy::InsertedAt, SortDirection::Ascending) => "inserted_at_ns ASC, rowid ASC",
            (SortBy::InsertedAt, SortDirection::Descending) => "inserted_at_ns DESC, rowid DESC",
        }
    }

    fn sent_at_order(direction: &SortDirection) -> &'static str {
        match direction {
            SortDirection::Ascending => "sent_at_ns ASC",
            SortDirection::Descending => "sent_at_ns DESC",
        }
    }

    fn owned_ids(ids: &[&[u8]]) -> Vec<Vec<u8>> {
        ids.iter().map(|id| id.to_vec()).collect()
    }

    /// The body of `get_group_messages`, against a caller-supplied connection.
    ///
    /// Split out so `get_group_messages_with_reactions` can run it and its
    /// reactions query on one connection, the way the sync path does, instead of
    /// acquiring twice.
    async fn fetch_group_messages(
        conn: &mut sqlx::PgConnection,
        group_id: &GroupId,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
        let filters = MsgFilters::new(args);
        let sql = format!(
            "SELECT {cols} FROM group_messages WHERE {group} AND {MSG_FILTERS} \
             ORDER BY {order} LIMIT $12::bigint",
            cols = StoredGroupMessage::select_columns(),
            group = stitched_group_filter(11),
            order = message_order(args),
        );
        let query = sqlx::query_as::<_, StoredGroupMessage>(&sql);
        Ok(filters
            .bind(query)
            .bind(group_id)
            .bind(args.limit)
            .fetch_all(conn)
            .await?)
    }

    /// Shared insert for `Store`/`StoreOrIgnore`, mirroring the diesel custom
    /// impls that route through `NewStoredGroupMessage`: every column except
    /// `inserted_at_ns` (DB-set) is written, in struct field order.
    async fn insert_message(
        m: &StoredGroupMessage,
        into: &impl crate::PgConnectionProvider,
        on_conflict_ignore: bool,
    ) -> Result<(), crate::StorageError> {
        let conflict = if on_conflict_ignore {
            " ON CONFLICT DO NOTHING"
        } else {
            ""
        };
        let sql = format!(
            "INSERT INTO group_messages \
             (id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, \
              sender_inbox_id, delivery_status, content_type, version_major, version_minor, \
              authority_id, reference_id, originator_id, sequence_id, expire_at_ns, should_push, \
              idempotency_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
             $18){conflict}"
        );
        let mut c = into.pg_conn().await?;
        sqlx::query(&sql)
            .bind(&m.id)
            .bind(m.group_id)
            .bind(&m.decrypted_message_bytes)
            .bind(m.sent_at_ns)
            .bind(m.kind)
            .bind(&m.sender_installation_id)
            .bind(&m.sender_inbox_id)
            .bind(m.delivery_status)
            .bind(m.content_type)
            .bind(m.version_major)
            .bind(m.version_minor)
            .bind(&m.authority_id)
            .bind(&m.reference_id)
            .bind(m.originator_id)
            .bind(m.sequence_id)
            .bind(m.expire_at_ns)
            .bind(m.should_push)
            .bind(&m.idempotency_key)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    impl<C: crate::PgConnectionProvider> crate::Store<C> for StoredGroupMessage {
        type Output = ();
        async fn store(&self, into: &C) -> Result<(), crate::StorageError> {
            insert_message(self, into, false).await
        }
    }

    impl<C: crate::PgConnectionProvider> crate::StoreOrIgnore<C> for StoredGroupMessage {
        type Output = ();
        async fn store_or_ignore(&self, into: &C) -> Result<(), crate::StorageError> {
            insert_message(self, into, true).await
        }
    }

    impl QueryGroupMessage for PgDb {
        async fn get_group_messages(
            &self,
            group_id: &GroupId,
            args: &MsgQueryArgs,
        ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            fetch_group_messages(&mut c, group_id, args).await
        }

        async fn count_group_messages(
            &self,
            group_id: &GroupId,
            args: &MsgQueryArgs,
        ) -> Result<i64, crate::ConnectionError> {
            // One connection for the conversation-type probe and the count, as
            // the sync path has.
            let mut c = self.conn().await?;

            let is_dm: ConversationType =
                sqlx::query_scalar("SELECT conversation_type FROM groups WHERE id = $1")
                    .bind(group_id)
                    .fetch_one(&mut *c)
                    .await?;
            let is_dm = is_dm == ConversationType::Dm;

            let include_group_updated = args
                .content_types
                .as_ref()
                .map(|types| types.contains(&ContentType::GroupUpdated))
                .unwrap_or(false);

            // DMs accumulate duplicate GroupUpdated messages that the listing
            // path dedupes after the fact. A count cannot, so they are excluded
            // outright unless asked for.
            let excluded =
                (is_dm && !include_group_updated).then_some(ContentType::GroupUpdated as i32);

            let filters = MsgFilters::new(args);
            let sql = format!(
                "SELECT COUNT(*) FROM group_messages \
                 WHERE {group} AND ($12::int4 IS NULL OR content_type <> $12) AND {MSG_FILTERS}",
                group = stitched_group_filter(11),
            );
            let query = sqlx::query_as::<_, (i64,)>(&sql);
            let (count,) = filters
                .bind(query)
                .bind(group_id)
                .bind(excluded)
                .fetch_one(&mut *c)
                .await?;
            Ok(count)
        }

        /// `sequence_id` is NOT NULL in the Postgres schema, so the sync path's
        /// `is_not_null()` guard has no analogue and is dropped.
        async fn missing_messages(
            &self,
            group_id: &GroupId,
            sequence_ids: &[u64],
        ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
            let sequence_ids: Vec<i64> = sequence_ids.iter().map(|id| *id as i64).collect();
            let sql = format!(
                "SELECT {} FROM group_messages \
                 WHERE group_id = $1 AND sequence_id <> ALL($2::bigint[]) AND kind = $3 \
                 ORDER BY sequence_id ASC",
                StoredGroupMessage::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(&sequence_ids)
                .bind(GroupMessageKind::Application)
                .fetch_all(&mut *c)
                .await?)
        }

        async fn group_messages_paged(
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

            // `exclude_disappearing` drops every message with an expiry;
            // otherwise only the already-expired ones are hidden, which needs a
            // "now" bind the other arm has no use for.
            let expiry = if *exclude_disappearing {
                "m.expire_at_ns IS NULL"
            } else {
                "(m.expire_at_ns IS NULL OR m.expire_at_ns > $7)"
            };
            let sql = format!(
                "SELECT {cols} FROM group_messages m LEFT JOIN groups g ON m.group_id = g.id \
                 WHERE g.conversation_type <> ALL($1::int4[]) \
                   AND m.kind = $2 \
                   AND ($3::bigint IS NULL OR m.sent_at_ns > $3) \
                   AND ($4::bigint IS NULL OR m.sent_at_ns <= $4) \
                   AND {expiry} \
                 ORDER BY m.id LIMIT $5 OFFSET $6",
                cols = StoredGroupMessage::select_columns_for("m"),
            );

            let virtual_types: Vec<i32> = ConversationType::virtual_types()
                .into_iter()
                .map(|t| t as i32)
                .collect();
            let mut query = sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(virtual_types)
                .bind(GroupMessageKind::Application)
                .bind(sent_after_ns)
                .bind(sent_before_ns)
                .bind(limit.unwrap_or(100))
                .bind(offset);
            if !*exclude_disappearing {
                query = query.bind(now_ns());
            }

            let mut c = self.conn().await?;
            Ok(query.fetch_all(&mut *c).await?)
        }

        async fn get_group_messages_with_reactions(
            &self,
            group_id: &GroupId,
            args: &MsgQueryArgs,
        ) -> Result<Vec<StoredGroupMessageWithReactions>, crate::ConnectionError> {
            // Reactions are fetched separately below, so they must not also come
            // back in the main list.
            let mut modified_args = args.clone();
            modified_args.content_types = match args.content_types.clone() {
                Some(mut content_types) => {
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

            let mut c = self.conn().await?;
            let messages = fetch_group_messages(&mut c, group_id, &modified_args).await?;

            let message_ids: Vec<Vec<u8>> = messages.iter().map(|m| m.id.clone()).collect();
            let sql = format!(
                "SELECT {cols} FROM group_messages \
                 WHERE {group} AND reference_id IS NOT NULL AND reference_id = ANY($2::bytea[]) \
                 ORDER BY {order}",
                cols = StoredGroupMessage::select_columns(),
                group = stitched_group_filter(1),
                order = sent_at_order(args.direction.as_ref().unwrap_or(&SortDirection::Ascending)),
            );
            let reactions = sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(&message_ids)
                .fetch_all(&mut *c)
                .await?;

            let mut reactions_by_reference: HashMap<Vec<u8>, Vec<StoredGroupMessage>> =
                HashMap::new();
            for reaction in reactions {
                if let Some(reference_id) = &reaction.reference_id {
                    reactions_by_reference
                        .entry(reference_id.clone())
                        .or_default()
                        .push(reaction);
                }
            }

            Ok(messages
                .into_iter()
                .map(|message| StoredGroupMessageWithReactions {
                    reactions: reactions_by_reference
                        .remove(&message.id)
                        .unwrap_or_default(),
                    message,
                })
                .collect())
        }

        async fn get_inbound_relations(
            &self,
            group_id: &GroupId,
            message_ids: &[&[u8]],
            relation_query: RelationQuery,
        ) -> Result<InboundRelations, crate::ConnectionError> {
            let sql = format!(
                "SELECT {cols} FROM group_messages \
                 WHERE {group} AND reference_id IS NOT NULL AND reference_id = ANY($2::bytea[]) \
                   AND ($3::int4[] IS NULL OR content_type = ANY($3)) \
                 ORDER BY {order} LIMIT $4::bigint",
                cols = StoredGroupMessage::select_columns(),
                group = stitched_group_filter(1),
                order = sent_at_order(&relation_query.direction),
            );
            let mut c = self.conn().await?;
            let relations = sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(owned_ids(message_ids))
                .bind(as_ints(&relation_query.content_types))
                .bind(relation_query.limit)
                .fetch_all(&mut *c)
                .await?;

            let mut inbound: InboundRelations = HashMap::new();
            for relation in relations {
                if let Some(reference_id) = &relation.reference_id {
                    inbound
                        .entry(reference_id.clone())
                        .or_default()
                        .push(relation);
                }
            }
            Ok(inbound)
        }

        async fn get_outbound_relations(
            &self,
            group_id: &GroupId,
            reference_ids: &[&[u8]],
        ) -> Result<OutboundRelations, crate::ConnectionError> {
            let sql = format!(
                "SELECT {cols} FROM group_messages WHERE {group} AND id = ANY($2::bytea[])",
                cols = StoredGroupMessage::select_columns(),
                group = stitched_group_filter(1),
            );
            let mut c = self.conn().await?;
            let referenced = sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(owned_ids(reference_ids))
                .fetch_all(&mut *c)
                .await?;

            Ok(referenced
                .into_iter()
                .map(|message| (message.id.clone(), message))
                .collect())
        }

        async fn get_inbound_relation_counts(
            &self,
            group_id: &GroupId,
            message_ids: &[&[u8]],
            relation_query: RelationQuery,
        ) -> Result<RelationCounts, crate::ConnectionError> {
            // `reference_id` decodes as non-null because the filter guarantees it.
            let sql = format!(
                "SELECT reference_id, COUNT(*) FROM group_messages \
                 WHERE {group} AND reference_id IS NOT NULL AND reference_id = ANY($2::bytea[]) \
                   AND ($3::int4[] IS NULL OR content_type = ANY($3)) \
                 GROUP BY reference_id",
                group = stitched_group_filter(1),
            );
            let mut c = self.conn().await?;
            let counts: Vec<(Vec<u8>, i64)> = sqlx::query_as(&sql)
                .bind(group_id)
                .bind(owned_ids(message_ids))
                .bind(as_ints(&relation_query.content_types))
                .fetch_all(&mut *c)
                .await?;

            Ok(counts
                .into_iter()
                .map(|(reference_id, count)| (reference_id, count as usize))
                .collect())
        }

        /// `MAX` over a NOT NULL column within a group is never NULL, so unlike
        /// the sync path there is nothing to filter out afterwards.
        async fn get_latest_message_times_by_sender(
            &self,
            group_id: &GroupId,
            allowed_content_types: &[ContentType],
        ) -> Result<LatestMessageTimeBySender, crate::ConnectionError> {
            let sql = format!(
                "SELECT sender_inbox_id, MAX(sent_at_ns) FROM group_messages \
                 WHERE {group} AND content_type = ANY($2::int4[]) \
                 GROUP BY sender_inbox_id",
                group = stitched_group_filter(1),
            );
            let types: Vec<i32> = allowed_content_types.iter().map(|t| *t as i32).collect();
            let mut c = self.conn().await?;
            let rows: Vec<(String, i64)> = sqlx::query_as(&sql)
                .bind(group_id)
                .bind(&types)
                .fetch_all(&mut *c)
                .await?;
            Ok(rows.into_iter().collect())
        }

        async fn get_group_message(
            &self,
            id: &[u8],
        ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM group_messages WHERE id = $1",
                StoredGroupMessage::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(id)
                .fetch_optional(&mut *c)
                .await?)
        }

        /// The read/write connection split is a SQLite backend concept (one SQLite
        /// writer, many readers); a Postgres pool has no such distinction, so
        /// this is `get_group_message`.
        async fn write_conn_get_group_message(
            &self,
            id: &[u8],
        ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
            self.get_group_message(id).await
        }

        async fn get_group_message_by_timestamp(
            &self,
            group_id: &GroupId,
            timestamp: i64,
        ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM group_messages WHERE group_id = $1 AND sent_at_ns = $2 LIMIT 1",
                StoredGroupMessage::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(timestamp)
                .fetch_optional(&mut *c)
                .await?)
        }

        async fn get_group_message_by_cursor(
            &self,
            group_id: &GroupId,
            cursor: Cursor,
        ) -> Result<Option<StoredGroupMessage>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM group_messages \
                 WHERE group_id = $1 AND sequence_id = $2 AND originator_id = $3 LIMIT 1",
                StoredGroupMessage::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(group_id)
                .bind(cursor.sequence_id as i64)
                .bind(cursor.originator_id as i64)
                .fetch_optional(&mut *c)
                .await?)
        }

        async fn set_delivery_status_to_published(
            &self,
            msg_id: &[u8],
            timestamp: u64,
            cursor: Cursor,
            message_expire_at_ns: Option<i64>,
        ) -> Result<usize, crate::ConnectionError> {
            tracing::info!(
                "Message [{}] published with cursor = {}",
                hex::encode(msg_id),
                cursor
            );
            let mut c = self.conn().await?;
            let updated = sqlx::query(
                "UPDATE group_messages SET delivery_status = $1, sent_at_ns = $2, \
                 sequence_id = $3, originator_id = $4, expire_at_ns = $5 WHERE id = $6",
            )
            .bind(DeliveryStatus::Published)
            .bind(timestamp as i64)
            .bind(cursor.sequence_id as i64)
            .bind(cursor.originator_id as i64)
            .bind(message_expire_at_ns)
            .bind(msg_id)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(updated as usize)
        }

        async fn set_delivery_status_to_failed(
            &self,
            msg_id: &[u8],
        ) -> Result<usize, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let updated =
                sqlx::query("UPDATE group_messages SET delivery_status = $1 WHERE id = $2")
                    .bind(DeliveryStatus::Failed)
                    .bind(msg_id)
                    .execute(&mut *c)
                    .await?
                    .rows_affected();
            Ok(updated as usize)
        }

        async fn delete_expired_messages(
            &self,
        ) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError> {
            let sql = format!(
                "DELETE FROM group_messages \
                 WHERE delivery_status = $1 AND kind = $2 \
                   AND expire_at_ns IS NOT NULL AND expire_at_ns <= $3 \
                 RETURNING {}",
                StoredGroupMessage::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupMessage>(&sql)
                .bind(DeliveryStatus::Published)
                .bind(GroupMessageKind::Application)
                .bind(now_ns())
                .fetch_all(&mut *c)
                .await?)
        }

        /// A true aggregate `MIN` -- not the two-argument scalar `MIN(a, b)` that
        /// SQLite has and Postgres spells `LEAST`. With no GROUP BY it always
        /// returns one row, NULL when nothing matches.
        async fn min_expire_at_ns(&self) -> Result<Option<i64>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            Ok(sqlx::query_scalar(
                "SELECT MIN(expire_at_ns) FROM group_messages \
                 WHERE delivery_status = $1 AND kind = $2 AND expire_at_ns IS NOT NULL",
            )
            .bind(DeliveryStatus::Published)
            .bind(GroupMessageKind::Application)
            .fetch_one(&mut *c)
            .await?)
        }

        async fn delete_message_by_id(
            &self,
            message_id: &[u8],
        ) -> Result<usize, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let deleted = sqlx::query("DELETE FROM group_messages WHERE id = $1")
                .bind(message_id)
                .execute(&mut *c)
                .await?
                .rows_affected();
            Ok(deleted as usize)
        }

        /// One statement for every group, where the sync path builds a tree of
        /// boxed `OR`s and chunks into batches of 100 to stay under SQLite's bind
        /// ceiling. Postgres takes the whole set as four array parameters.
        ///
        /// Per group with a cursor, the sync predicate is "some known originator
        /// is behind, or the originator is unknown". Its contrapositive is the
        /// single `NOT EXISTS` below: no cursor entry for this message's
        /// originator is at or ahead of it.
        async fn messages_newer_than(
            &self,
            cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
        ) -> Result<Vec<(GroupId, Cursor)>, crate::ConnectionError> {
            let mut uncursored: Vec<Vec<u8>> = Vec::new();
            let mut cursored: Vec<Vec<u8>> = Vec::new();
            let (mut group_ids, mut originator_ids, mut sequence_ids) =
                (Vec::new(), Vec::new(), Vec::new());

            for (group_id, global_cursor) in cursors_by_group {
                if global_cursor.is_empty() {
                    uncursored.push(group_id.clone());
                    continue;
                }
                cursored.push(group_id.clone());
                for (originator_id, sequence_id) in global_cursor.iter() {
                    group_ids.push(group_id.clone());
                    originator_ids.push(*originator_id as i64);
                    sequence_ids.push(*sequence_id as i64);
                }
            }

            let mut c = self.conn().await?;
            let rows: Vec<(GroupId, i64, i64)> = sqlx::query_as(
                "SELECT m.group_id, m.originator_id, m.sequence_id FROM group_messages m \
                 WHERE m.group_id = ANY($1::bytea[]) \
                    OR (m.group_id = ANY($2::bytea[]) AND NOT EXISTS ( \
                          SELECT 1 FROM UNNEST($3::bytea[], $4::bigint[], $5::bigint[]) \
                                       AS seen(group_id, originator_id, sequence_id) \
                          WHERE seen.group_id = m.group_id \
                            AND seen.originator_id = m.originator_id \
                            AND m.sequence_id <= seen.sequence_id))",
            )
            .bind(&uncursored)
            .bind(&cursored)
            .bind(&group_ids)
            .bind(&originator_ids)
            .bind(&sequence_ids)
            .fetch_all(&mut *c)
            .await?;

            Ok(rows
                .into_iter()
                .map(|(group_id, originator_id, sequence_id)| {
                    (
                        group_id,
                        Cursor::new(sequence_id as u64, originator_id as u32),
                    )
                })
                .collect())
        }

        async fn clear_messages(
            &self,
            group_ids: Option<&[GroupId]>,
            retention_days: Option<u32>,
        ) -> Result<usize, crate::ConnectionError> {
            let group_ids: Option<Vec<Vec<u8>>> =
                group_ids.map(|ids| ids.iter().map(|id| id.to_vec()).collect());
            let cutoff_ns = retention_days
                .map(|days| now_ns().saturating_sub(NS_IN_DAY.saturating_mul(i64::from(days))));

            let mut c = self.conn().await?;
            let deleted = sqlx::query(
                "DELETE FROM group_messages \
                 WHERE ($1::bytea[] IS NULL OR group_id = ANY($1)) \
                   AND ($2::bigint IS NULL OR sent_at_ns < $2)",
            )
            .bind(&group_ids)
            .bind(cutoff_ns)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(deleted as usize)
        }
    }
}
