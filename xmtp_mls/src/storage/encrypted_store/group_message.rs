use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use xmtp_content_types::{
    attachment, group_updated, membership_change, reaction, read_receipt, remote_attachment, reply,
    text, transaction_reference,
};

use super::{
    db_connection::DbConnection,
    schema::group_messages::{self, dsl},
    Sqlite,
};
use crate::{impl_fetch, impl_store, impl_store_or_ignore, StorageError};

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

#[derive(Default)]
pub struct MsgQueryArgs {
    sent_after_ns: Option<i64>,
    sent_before_ns: Option<i64>,
    kind: Option<GroupMessageKind>,
    delivery_status: Option<DeliveryStatus>,
    limit: Option<i64>,
    direction: Option<SortDirection>,
    content_types: Option<Vec<ContentType>>,
}

impl MsgQueryArgs {
    pub fn sent_after_ns(mut self, sent_after_ns: i64) -> Self {
        self.sent_after_ns = Some(sent_after_ns);
        self
    }
    pub fn maybe_sent_after_ns(mut self, sent_after_ns: Option<i64>) -> Self {
        self.sent_after_ns = sent_after_ns;
        self
    }

    pub fn sent_before_ns(mut self, sent_before_ns: i64) -> Self {
        self.sent_before_ns = Some(sent_before_ns);
        self
    }
    pub fn maybe_sent_before_ns(mut self, sent_before_ns: Option<i64>) -> Self {
        self.sent_before_ns = sent_before_ns;
        self
    }

    pub fn kind(mut self, kind: GroupMessageKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn maybe_kind(mut self, kind: Option<GroupMessageKind>) -> Self {
        self.kind = kind;
        self
    }

    pub fn direction(mut self, direction: SortDirection) -> Self {
        self.direction = Some(direction);
        self
    }
    pub fn maybe_direction(mut self, direction: Option<SortDirection>) -> Self {
        self.direction = direction;
        self
    }

    pub fn delivery_status(mut self, delivery_status: DeliveryStatus) -> Self {
        self.delivery_status = Some(delivery_status);
        self
    }
    pub fn maybe_delivery_status(mut self, delivery_status: Option<DeliveryStatus>) -> Self {
        self.delivery_status = delivery_status;
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }
    pub fn maybe_limit(mut self, limit: Option<i64>) -> Self {
        self.limit = limit;
        self
    }

    pub fn content_types(mut self, content_types: Vec<ContentType>) -> Self {
        self.content_types = Some(content_types);
        self
    }

    pub fn maybe_content_types(mut self, content_types: Option<Vec<ContentType>>) -> Self {
        self.content_types = content_types;
        self
    }
}

impl DbConnection {
    /// Query for group messages
    #[allow(clippy::too_many_arguments)]
    pub fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let mut query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
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

        Ok(self.raw_query(|conn| query.load::<StoredGroupMessage>(conn))?)
    }

    /// Get a particular group message
    pub fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        Ok(self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first(conn)
                .optional()
        })?)
    }

    pub fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        Ok(self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id.as_ref()))
                .filter(dsl::sent_at_ns.eq(timestamp))
                .first(conn)
                .optional()
        })?)
    }

    pub fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((
                    dsl::delivery_status.eq(DeliveryStatus::Published),
                    dsl::sent_at_ns.eq(timestamp as i64),
                ))
                .execute(conn)
        })?)
    }

    pub fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((dsl::delivery_status.eq(DeliveryStatus::Failed),))
                .execute(conn)
        })?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        storage::encrypted_store::{group::tests::generate_group, tests::with_connection},
        Store,
    };
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_common::{assert_err, assert_ok, rand_time, rand_vec};

    fn generate_message(
        kind: Option<GroupMessageKind>,
        group_id: Option<&[u8]>,
        sent_at_ns: Option<i64>,
        content_type: Option<ContentType>,
    ) -> StoredGroupMessage {
        StoredGroupMessage {
            id: rand_vec::<24>(),
            group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_vec::<24>()),
            decrypted_message_bytes: rand_vec::<24>(),
            sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
            sender_installation_id: rand_vec::<24>(),
            sender_inbox_id: "0x0".to_string(),
            kind: kind.unwrap_or(GroupMessageKind::Application),
            delivery_status: DeliveryStatus::Unpublished,
            content_type: content_type.unwrap_or(ContentType::Unknown),
            version_major: 0,
            version_minor: 0,
            authority_id: "unknown".to_string(),
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_does_not_error_on_empty_messages() {
        with_connection(|conn| {
            let id = vec![0x0];
            assert_eq!(conn.get_group_message(id).unwrap(), None);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages() {
        with_connection(|conn| {
            let group = generate_group(None);
            let message = generate_message(None, Some(&group.id), None, None);
            group.store(conn).unwrap();
            let id = message.id.clone();

            message.store(conn).unwrap();

            let stored_message = conn.get_group_message(id);
            assert_eq!(stored_message.unwrap(), Some(message));
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_cannot_insert_message_without_group() {
        use diesel::result::{DatabaseErrorKind::ForeignKeyViolation, Error::DatabaseError};

        with_connection(|conn| {
            let message = generate_message(None, None, None, None);
            assert_err!(
                message.store(conn),
                StorageError::DieselResult(DatabaseError(ForeignKeyViolation, _))
            );
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_many_messages() {
        use crate::storage::encrypted_store::schema::group_messages::dsl;

        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            for idx in 0..50 {
                let msg = generate_message(None, Some(&group.id), Some(idx), None);
                assert_ok!(msg.store(conn));
            }

            let count: i64 = conn
                .raw_query(|raw_conn| {
                    dsl::group_messages
                        .select(diesel::dsl::count_star())
                        .first(raw_conn)
                })
                .unwrap();
            assert_eq!(count, 50);

            let messages = conn
                .get_group_messages(&group.id, &MsgQueryArgs::default())
                .unwrap();

            assert_eq!(messages.len(), 50);
            messages.iter().fold(0, |acc, msg| {
                assert!(msg.sent_at_ns >= acc);
                msg.sent_at_ns
            });
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages_by_time() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(1_000), None),
                generate_message(None, Some(&group.id), Some(100_000), None),
                generate_message(None, Some(&group.id), Some(10_000), None),
                generate_message(None, Some(&group.id), Some(1_000_000), None),
            ];
            assert_ok!(messages.store(conn));
            let message = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default()
                        .sent_after_ns(1_000)
                        .sent_before_ns(100_000),
                )
                .unwrap();
            assert_eq!(message.len(), 1);
            assert_eq!(message.first().unwrap().sent_at_ns, 10_000);

            let messages = conn
                .get_group_messages(&group.id, &MsgQueryArgs::default().sent_before_ns(100_000))
                .unwrap();
            assert_eq!(messages.len(), 2);

            let messages = conn
                .get_group_messages(&group.id, &MsgQueryArgs::default().sent_after_ns(10_000))
                .unwrap();
            assert_eq!(messages.len(), 2);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages_by_kind() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            // just a bunch of random messages so we have something to filter through
            for i in 0..30 {
                match i % 2 {
                    0 => {
                        let msg = generate_message(
                            Some(GroupMessageKind::Application),
                            Some(&group.id),
                            None,
                            Some(ContentType::Text),
                        );
                        msg.store(conn).unwrap();
                    }
                    _ => {
                        let msg = generate_message(
                            Some(GroupMessageKind::MembershipChange),
                            Some(&group.id),
                            None,
                            Some(ContentType::GroupMembershipChange),
                        );
                        msg.store(conn).unwrap();
                    }
                }
            }

            let application_messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().kind(GroupMessageKind::Application),
                )
                .unwrap();
            assert_eq!(application_messages.len(), 15);

            let membership_changes = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().kind(GroupMessageKind::MembershipChange),
                )
                .unwrap();
            assert_eq!(membership_changes.len(), 15);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_orders_messages_by_sent() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(10_000), None),
                generate_message(None, Some(&group.id), Some(1_000), None),
                generate_message(None, Some(&group.id), Some(100_000), None),
                generate_message(None, Some(&group.id), Some(1_000_000), None),
            ];

            assert_ok!(messages.store(conn));

            let messages_asc = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().direction(SortDirection::Ascending),
                )
                .unwrap();
            assert_eq!(messages_asc.len(), 4);
            assert_eq!(messages_asc[0].sent_at_ns, 1_000);
            assert_eq!(messages_asc[1].sent_at_ns, 10_000);
            assert_eq!(messages_asc[2].sent_at_ns, 100_000);
            assert_eq!(messages_asc[3].sent_at_ns, 1_000_000);

            let messages_desc = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().direction(SortDirection::Descending),
                )
                .unwrap();
            assert_eq!(messages_desc.len(), 4);
            assert_eq!(messages_desc[0].sent_at_ns, 1_000_000);
            assert_eq!(messages_desc[1].sent_at_ns, 100_000);
            assert_eq!(messages_desc[2].sent_at_ns, 10_000);
            assert_eq!(messages_desc[3].sent_at_ns, 1_000);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages_by_content_type() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(1_000), Some(ContentType::Text)),
                generate_message(
                    None,
                    Some(&group.id),
                    Some(2_000),
                    Some(ContentType::GroupMembershipChange),
                ),
                generate_message(
                    None,
                    Some(&group.id),
                    Some(3_000),
                    Some(ContentType::GroupUpdated),
                ),
            ];
            assert_ok!(messages.store(conn));

            // Query for text messages
            let text_messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().content_types(vec![ContentType::Text]),
                )
                .unwrap();
            assert_eq!(text_messages.len(), 1);
            assert_eq!(text_messages[0].content_type, ContentType::Text);
            assert_eq!(text_messages[0].sent_at_ns, 1_000);

            // Query for membership change messages
            let membership_messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default()
                        .content_types(vec![ContentType::GroupMembershipChange]),
                )
                .unwrap();
            assert_eq!(membership_messages.len(), 1);
            assert_eq!(
                membership_messages[0].content_type,
                ContentType::GroupMembershipChange
            );
            assert_eq!(membership_messages[0].sent_at_ns, 2_000);

            // Query for group updated messages
            let updated_messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs::default().content_types(vec![ContentType::GroupUpdated]),
                )
                .unwrap();
            assert_eq!(updated_messages.len(), 1);
            assert_eq!(updated_messages[0].content_type, ContentType::GroupUpdated);
            assert_eq!(updated_messages[0].sent_at_ns, 3_000);
        })
        .await
    }
}
