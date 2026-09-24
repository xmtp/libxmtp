use xmtp_db::group::{GroupQueryArgs, GroupQueryOrderBy};
use xmtp_proto::types::ConversationType;

use crate::{
    ConsentState, ContentTypeId, DeliveryStatus, DisappearingSettings, InboxID, MessageKind,
    PermissionPolicySet, Timestamp, XmtpError,
};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum GroupPermissionMode {
    AllMembers,
    AdminOnly,
    Custom { policy_set: PermissionPolicySet },
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct CreateGroupOptions {
    #[uniffi(default = None)]
    pub permissions: Option<GroupPermissionMode>,
    #[uniffi(default = None)]
    pub name: Option<String>,
    #[uniffi(default = None)]
    pub image_url: Option<String>,
    #[uniffi(default = None)]
    pub description: Option<String>,
    #[uniffi(default = None)]
    pub disappearing: Option<DisappearingSettings>,
    #[uniffi(default = None)]
    pub app_data: Option<String>,
}

impl CreateGroupOptions {
    pub(crate) fn into_core(
        self,
    ) -> Result<
        (
            Option<xmtp_mls::groups::group_permissions::PolicySet>,
            xmtp_mls::mls_common::group::GroupMetadataOptions,
        ),
        XmtpError,
    > {
        let permissions = match self.permissions {
            None => None,
            Some(GroupPermissionMode::AllMembers) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::Default.to_policy_set())
            }
            Some(GroupPermissionMode::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly.to_policy_set())
            }
            Some(GroupPermissionMode::Custom { policy_set }) => Some(policy_set.try_into()?),
        };
        Ok((
            permissions,
            xmtp_mls::mls_common::group::GroupMetadataOptions {
                name: self.name,
                image_url_square: self.image_url,
                description: self.description,
                message_disappearing_settings: self.disappearing.map(Into::into),
                app_data: self.app_data,
            },
        ))
    }
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct CreateDmOptions {
    #[uniffi(default = None)]
    pub disappearing: Option<DisappearingSettings>,
}

impl From<CreateDmOptions> for xmtp_mls::mls_common::group::DMMetadataOptions {
    fn from(value: CreateDmOptions) -> Self {
        Self {
            message_disappearing_settings: value.disappearing.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageSortBy {
    SentAt,
    InsertedAt,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct ListMessagesOptions {
    #[uniffi(default = None)]
    pub limit: Option<u32>,
    #[uniffi(default = None)]
    pub sent_before: Option<Timestamp>,
    #[uniffi(default = None)]
    pub sent_after: Option<Timestamp>,
    #[uniffi(default = None)]
    pub inserted_before: Option<Timestamp>,
    #[uniffi(default = None)]
    pub inserted_after: Option<Timestamp>,
    #[uniffi(default = None)]
    pub direction: Option<MessageOrder>,
    #[uniffi(default = None)]
    pub sort_by: Option<MessageSortBy>,
    #[uniffi(default = None)]
    pub delivery_status: Option<DeliveryStatus>,
    #[uniffi(default = None)]
    pub kind: Option<MessageKind>,
    #[uniffi(default = None)]
    pub content_types: Option<Vec<ContentTypeId>>,
    #[uniffi(default = None)]
    pub exclude_content_types: Option<Vec<ContentTypeId>>,
    #[uniffi(default = None)]
    pub exclude_sender_inbox_ids: Option<Vec<InboxID>>,
}

impl TryFrom<ListMessagesOptions> for xmtp_db::group_message::MsgQueryArgs {
    type Error = XmtpError;

    fn try_from(value: ListMessagesOptions) -> Result<Self, Self::Error> {
        use xmtp_db::group_message::{
            DeliveryStatus as StoredStatus, GroupMessageKind, SortBy, SortDirection,
        };
        Ok(Self {
            limit: value.limit.map(i64::from),
            sent_before_ns: value.sent_before.map(|time| time.0),
            sent_after_ns: value.sent_after.map(|time| time.0),
            inserted_before_ns: value.inserted_before.map(|time| time.0),
            inserted_after_ns: value.inserted_after.map(|time| time.0),
            direction: value.direction.map(|direction| match direction {
                MessageOrder::Ascending => SortDirection::Ascending,
                MessageOrder::Descending => SortDirection::Descending,
            }),
            sort_by: value.sort_by.map(|sort| match sort {
                MessageSortBy::SentAt => SortBy::SentAt,
                MessageSortBy::InsertedAt => SortBy::InsertedAt,
            }),
            delivery_status: value.delivery_status.map(|status| match status {
                DeliveryStatus::Unpublished => StoredStatus::Unpublished,
                DeliveryStatus::Published => StoredStatus::Published,
                DeliveryStatus::Failed => StoredStatus::Failed,
            }),
            kind: value.kind.map(|kind| match kind {
                MessageKind::Application => GroupMessageKind::Application,
                MessageKind::MembershipChange => GroupMessageKind::MembershipChange,
            }),
            content_types: value
                .content_types
                .map(crate::conversation::query_content_types)
                .transpose()?,
            exclude_content_types: value
                .exclude_content_types
                .map(crate::conversation::query_content_types)
                .transpose()?,
            exclude_sender_inbox_ids: value
                .exclude_sender_inbox_ids
                .map(|ids| ids.into_iter().map(|id| id.0).collect()),
            ..Default::default()
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ConversationKind {
    Group,
    Dm,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ConversationOrder {
    CreatedAt,
    LastActivity,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct ListConversationsOptions {
    #[uniffi(default = None)]
    pub kind: Option<ConversationKind>,
    #[uniffi(default = None)]
    pub consent_states: Option<Vec<ConsentState>>,
    #[uniffi(default = None)]
    pub created_after: Option<Timestamp>,
    #[uniffi(default = None)]
    pub created_before: Option<Timestamp>,
    #[uniffi(default = None)]
    pub last_activity_after: Option<Timestamp>,
    #[uniffi(default = None)]
    pub last_activity_before: Option<Timestamp>,
    #[uniffi(default = None)]
    pub limit: Option<u32>,
    #[uniffi(default = None)]
    pub order_by: Option<ConversationOrder>,
    #[uniffi(default = false)]
    pub include_duplicate_dms: bool,
}

impl From<ListConversationsOptions> for GroupQueryArgs {
    fn from(value: ListConversationsOptions) -> Self {
        Self {
            conversation_type: value.kind.map(|kind| match kind {
                ConversationKind::Group => ConversationType::Group,
                ConversationKind::Dm => ConversationType::Dm,
            }),
            consent_states: value
                .consent_states
                .map(|states| states.into_iter().map(Into::into).collect()),
            created_after_ns: value.created_after.map(|value| value.0),
            created_before_ns: value.created_before.map(|value| value.0),
            last_activity_after_ns: value.last_activity_after.map(|value| value.0),
            last_activity_before_ns: value.last_activity_before.map(|value| value.0),
            limit: value.limit.map(i64::from),
            order_by: value.order_by.map(|order| match order {
                ConversationOrder::CreatedAt => GroupQueryOrderBy::CreatedAt,
                ConversationOrder::LastActivity => GroupQueryOrderBy::LastActivity,
            }),
            include_duplicate_dms: value.include_duplicate_dms,
            ..Default::default()
        }
    }
}
