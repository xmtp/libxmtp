use std::collections::HashMap;

use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId as ProtoContentTypeId, EncodedContent as ProtoEncodedContent,
};

use crate::ContentTypeId;

#[derive(Clone, Debug, uniffi::Record)]
pub struct Attachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub content: Vec<u8>,
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
    fn from(value: xmtp_content_types::attachment::Attachment) -> Self {
        Self {
            filename: value.filename,
            mime_type: value.mime_type,
            content: value.content,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteAttachment {
    pub url: String,
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub scheme: String,
    pub content_length: Option<u32>,
    pub filename: Option<String>,
}

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
    fn from(value: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
        Self {
            url: value.url,
            content_digest: value.content_digest,
            secret: value.secret,
            salt: value.salt,
            nonce: value.nonce,
            scheme: value.scheme,
            content_length: value.content_length,
            filename: value.filename,
        }
    }
}

impl From<RemoteAttachment>
    for xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo
{
    fn from(value: RemoteAttachment) -> Self {
        Self {
            url: value.url,
            content_digest: value.content_digest,
            secret: value.secret,
            salt: value.salt,
            nonce: value.nonce,
            scheme: value.scheme,
            content_length: value.content_length,
            filename: value.filename,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MultiRemoteAttachment {
    pub attachments: Vec<RemoteAttachment>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment>
    for MultiRemoteAttachment
{
    fn from(
        value: xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment,
    ) -> Self {
        Self {
            attachments: value.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TransactionMetadata {
    pub transaction_type: String,
    pub currency: String,
    pub amount: f64,
    pub decimals: u32,
    pub from_address: String,
    pub to_address: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TransactionReference {
    pub namespace: Option<String>,
    pub network_id: String,
    pub reference: String,
    pub metadata: Option<TransactionMetadata>,
}

impl From<xmtp_content_types::transaction_reference::TransactionReference>
    for TransactionReference
{
    fn from(value: xmtp_content_types::transaction_reference::TransactionReference) -> Self {
        Self {
            namespace: value.namespace,
            network_id: value.network_id,
            reference: value.reference,
            metadata: value.metadata.map(|metadata| TransactionMetadata {
                transaction_type: metadata.transaction_type,
                currency: metadata.currency,
                amount: metadata.amount,
                decimals: metadata.decimals,
                from_address: metadata.from_address,
                to_address: metadata.to_address,
            }),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletCallMetadata {
    pub description: String,
    pub transaction_type: String,
    pub extra: HashMap<String, String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletCall {
    pub to: Option<String>,
    pub data: Option<String>,
    pub value: Option<String>,
    pub gas: Option<String>,
    pub metadata: Option<WalletCallMetadata>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletSendCalls {
    pub version: String,
    pub chain_id: String,
    pub from: String,
    pub calls: Vec<WalletCall>,
    pub capabilities: Option<HashMap<String, String>>,
}

impl From<xmtp_content_types::wallet_send_calls::WalletSendCalls> for WalletSendCalls {
    fn from(value: xmtp_content_types::wallet_send_calls::WalletSendCalls) -> Self {
        Self {
            version: value.version,
            chain_id: value.chain_id,
            from: value.from,
            calls: value
                .calls
                .into_iter()
                .map(|call| WalletCall {
                    to: call.to,
                    data: call.data,
                    value: call.value,
                    gas: call.gas,
                    metadata: call.metadata.map(|metadata| WalletCallMetadata {
                        description: metadata.description,
                        transaction_type: metadata.transaction_type,
                        extra: metadata.extra,
                    }),
                })
                .collect(),
            capabilities: value.capabilities,
        }
    }
}

impl From<WalletSendCalls> for xmtp_content_types::wallet_send_calls::WalletSendCalls {
    fn from(value: WalletSendCalls) -> Self {
        use xmtp_content_types::wallet_send_calls::{
            WalletCall as CoreCall, WalletCallMetadata as CoreMetadata,
        };
        Self {
            version: value.version,
            chain_id: value.chain_id,
            from: value.from,
            calls: value
                .calls
                .into_iter()
                .map(|call| CoreCall {
                    to: call.to,
                    data: call.data,
                    value: call.value,
                    gas: call.gas,
                    metadata: call.metadata.map(|metadata| CoreMetadata {
                        description: metadata.description,
                        transaction_type: metadata.transaction_type,
                        extra: metadata.extra,
                    }),
                })
                .collect(),
            capabilities: value.capabilities,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Intent {
    pub id: String,
    pub action_id: String,
    pub metadata_json: Option<String>,
}

impl From<xmtp_content_types::intent::Intent> for Intent {
    fn from(value: xmtp_content_types::intent::Intent) -> Self {
        Self {
            id: value.id,
            action_id: value.action_id,
            metadata_json: value
                .metadata
                .and_then(|value| serde_json::to_string(&value).ok()),
        }
    }
}

impl TryFrom<Intent> for xmtp_content_types::intent::Intent {
    type Error = crate::XmtpError;

    fn try_from(value: Intent) -> Result<Self, Self::Error> {
        let metadata = value
            .metadata_json
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| crate::XmtpError::invalid(error.to_string()))
            })
            .transpose()?;
        Ok(Self {
            id: value.id,
            action_id: value.action_id,
            metadata,
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ActionStyle {
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub image_url: Option<String>,
    pub style: Option<ActionStyle>,
    pub expires_at: Option<crate::Timestamp>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Actions {
    pub id: String,
    pub description: String,
    pub actions: Vec<Action>,
    pub expires_at: Option<crate::Timestamp>,
}

impl From<xmtp_content_types::actions::Actions> for Actions {
    fn from(value: xmtp_content_types::actions::Actions) -> Self {
        use xmtp_content_types::actions::ActionStyle as CoreStyle;
        Self {
            id: value.id,
            description: value.description,
            expires_at: value
                .expires_at
                .map(|time| crate::Timestamp(time.timestamp_nanos_opt().unwrap_or(i64::MAX))),
            actions: value
                .actions
                .into_iter()
                .map(|action| Action {
                    id: action.id,
                    label: action.label,
                    image_url: action.image_url,
                    style: action.style.map(|style| match style {
                        CoreStyle::Primary => ActionStyle::Primary,
                        CoreStyle::Secondary => ActionStyle::Secondary,
                        CoreStyle::Danger => ActionStyle::Danger,
                    }),
                    expires_at: action.expires_at.map(|time| {
                        crate::Timestamp(time.timestamp_nanos_opt().unwrap_or(i64::MAX))
                    }),
                })
                .collect(),
        }
    }
}

impl From<Actions> for xmtp_content_types::actions::Actions {
    fn from(value: Actions) -> Self {
        use xmtp_content_types::actions::{Action as CoreAction, ActionStyle as CoreStyle};
        Self {
            id: value.id,
            description: value.description,
            expires_at: value
                .expires_at
                .map(|time| chrono::DateTime::from_timestamp_nanos(time.0)),
            actions: value
                .actions
                .into_iter()
                .map(|action| CoreAction {
                    id: action.id,
                    label: action.label,
                    image_url: action.image_url,
                    style: action.style.map(|style| match style {
                        ActionStyle::Primary => CoreStyle::Primary,
                        ActionStyle::Secondary => CoreStyle::Secondary,
                        ActionStyle::Danger => CoreStyle::Danger,
                    }),
                    expires_at: action
                        .expires_at
                        .map(|time| chrono::DateTime::from_timestamp_nanos(time.0)),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LeaveRequest {
    pub authenticated_note: Option<Vec<u8>>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DeletedBy {
    Sender,
    Admin { inbox_id: crate::InboxID },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DeletedMessage {
    pub deleted_by: DeletedBy,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MetadataFieldChange {
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupUpdated {
    pub initiated_by_inbox_id: crate::InboxID,
    pub added_inboxes: Vec<crate::InboxID>,
    pub removed_inboxes: Vec<crate::InboxID>,
    pub left_inboxes: Vec<crate::InboxID>,
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub added_admin_inboxes: Vec<crate::InboxID>,
    pub removed_admin_inboxes: Vec<crate::InboxID>,
    pub added_super_admin_inboxes: Vec<crate::InboxID>,
    pub removed_super_admin_inboxes: Vec<crate::InboxID>,
}

impl TryFrom<xmtp_proto::xmtp::mls::message_contents::GroupUpdated> for GroupUpdated {
    type Error = crate::XmtpError;
    fn try_from(
        value: xmtp_proto::xmtp::mls::message_contents::GroupUpdated,
    ) -> Result<Self, Self::Error> {
        fn ids(
            values: Vec<xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox>,
        ) -> Result<Vec<crate::InboxID>, crate::XmtpError> {
            values
                .into_iter()
                .map(|value| crate::InboxID::try_from(value.inbox_id))
                .collect()
        }
        Ok(Self {
            initiated_by_inbox_id: crate::InboxID::try_from(value.initiated_by_inbox_id)?,
            added_inboxes: ids(value.added_inboxes)?,
            removed_inboxes: ids(value.removed_inboxes)?,
            left_inboxes: ids(value.left_inboxes)?,
            metadata_field_changes: value
                .metadata_field_changes
                .into_iter()
                .map(|field| MetadataFieldChange {
                    field_name: field.field_name,
                    old_value: field.old_value,
                    new_value: field.new_value,
                })
                .collect(),
            added_admin_inboxes: ids(value.added_admin_inboxes)?,
            removed_admin_inboxes: ids(value.removed_admin_inboxes)?,
            added_super_admin_inboxes: ids(value.added_super_admin_inboxes)?,
            removed_super_admin_inboxes: ids(value.removed_super_admin_inboxes)?,
        })
    }
}

impl From<GroupUpdated> for xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
    fn from(value: GroupUpdated) -> Self {
        use xmtp_proto::xmtp::mls::message_contents::group_updated::{
            Inbox, MetadataFieldChange as ProtoChange,
        };
        fn inboxes(values: Vec<crate::InboxID>) -> Vec<Inbox> {
            values
                .into_iter()
                .map(|value| Inbox { inbox_id: value.0 })
                .collect()
        }
        Self {
            initiated_by_inbox_id: value.initiated_by_inbox_id.0,
            added_inboxes: inboxes(value.added_inboxes),
            removed_inboxes: inboxes(value.removed_inboxes),
            left_inboxes: inboxes(value.left_inboxes),
            added_admin_inboxes: inboxes(value.added_admin_inboxes),
            removed_admin_inboxes: inboxes(value.removed_admin_inboxes),
            added_super_admin_inboxes: inboxes(value.added_super_admin_inboxes),
            removed_super_admin_inboxes: inboxes(value.removed_super_admin_inboxes),
            metadata_field_changes: value
                .metadata_field_changes
                .into_iter()
                .map(|field| ProtoChange {
                    field_name: field.field_name,
                    old_value: field.old_value,
                    new_value: field.new_value,
                })
                .collect(),
        }
    }
}

/// Content at this boundary is uncompressed. Send options control wire compression.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EncodedContent {
    pub r#type: ContentTypeId,
    #[uniffi(default)]
    pub parameters: HashMap<String, String>,
    #[uniffi(default = None)]
    pub fallback: Option<String>,
    pub content: Vec<u8>,
}

/// A standard value that can be encoded without a client.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum StandardContent {
    Text(String),
    Markdown(String),
    ReadReceipt,
    Reaction {
        reference: crate::MessageID,
        reference_inbox_id: Option<crate::InboxID>,
        reaction: Reaction,
    },
    Attachment(Attachment),
    RemoteAttachment(RemoteAttachment),
    MultiRemoteAttachment(MultiRemoteAttachment),
    TransactionReference(TransactionReference),
    WalletSendCalls(WalletSendCalls),
    Actions(Actions),
    Intent(Intent),
    Reply {
        reference: crate::MessageID,
        reference_inbox_id: Option<crate::InboxID>,
        content: EncodedContent,
    },
    GroupUpdated(GroupUpdated),
    DeleteMessage {
        message_id: crate::MessageID,
    },
    LeaveRequest(LeaveRequest),
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum StandardContentKind {
    Text,
    Markdown,
    ReadReceipt,
    Reaction,
    Attachment,
    RemoteAttachment,
    MultiRemoteAttachment,
    TransactionReference,
    WalletSendCalls,
    Actions,
    Intent,
    Reply,
    GroupUpdated,
    DeleteMessage,
    LeaveRequest,
}

fn codec_error(error: xmtp_content_types::CodecError) -> crate::XmtpError {
    crate::XmtpError::invalid(error.to_string())
}

fn standard_type(kind: StandardContentKind) -> ProtoContentTypeId {
    use xmtp_content_types::ContentCodec;
    match kind {
        StandardContentKind::Text => xmtp_content_types::text::TextCodec::content_type(),
        StandardContentKind::Markdown => {
            xmtp_content_types::markdown::MarkdownCodec::content_type()
        }
        StandardContentKind::ReadReceipt => {
            xmtp_content_types::read_receipt::ReadReceiptCodec::content_type()
        }
        StandardContentKind::Reaction => {
            xmtp_content_types::reaction::ReactionCodec::content_type()
        }
        StandardContentKind::Attachment => {
            xmtp_content_types::attachment::AttachmentCodec::content_type()
        }
        StandardContentKind::RemoteAttachment => {
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::content_type()
        }
        StandardContentKind::MultiRemoteAttachment => {
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::content_type()
        }
        StandardContentKind::TransactionReference => {
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::content_type()
        }
        StandardContentKind::WalletSendCalls => {
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::content_type()
        }
        StandardContentKind::Actions => xmtp_content_types::actions::ActionsCodec::content_type(),
        StandardContentKind::Intent => xmtp_content_types::intent::IntentCodec::content_type(),
        StandardContentKind::Reply => xmtp_content_types::reply::ReplyCodec::content_type(),
        StandardContentKind::GroupUpdated => {
            xmtp_content_types::group_updated::GroupUpdatedCodec::content_type()
        }
        StandardContentKind::DeleteMessage => {
            xmtp_content_types::delete_message::DeleteMessageCodec::content_type()
        }
        StandardContentKind::LeaveRequest => {
            xmtp_content_types::leave_request::LeaveRequestCodec::content_type()
        }
    }
}

#[xmtp_macro::sdk_export(pure)]
pub fn standard_content_type(kind: StandardContentKind) -> ContentTypeId {
    let kind = standard_type(kind);
    ContentTypeId {
        authority_id: kind.authority_id,
        type_id: kind.type_id,
        version_major: kind.version_major,
        version_minor: kind.version_minor,
    }
}

#[xmtp_macro::sdk_export(pure)]
pub fn encode_standard(value: StandardContent) -> Result<EncodedContent, crate::XmtpError> {
    use xmtp_content_types::ContentCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types as proto;
    let encoded = match value {
        StandardContent::Text(value) => xmtp_content_types::text::TextCodec::encode(value),
        StandardContent::Markdown(value) => {
            xmtp_content_types::markdown::MarkdownCodec::encode(value)
        }
        StandardContent::ReadReceipt => xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
            xmtp_content_types::read_receipt::ReadReceipt {},
        ),
        StandardContent::Reaction {
            reference,
            reference_inbox_id,
            reaction,
        } => xmtp_content_types::reaction::ReactionCodec::encode(reaction.into_proto(
            reference,
            crate::InboxID(reference_inbox_id.map(|id| id.0).unwrap_or_default()),
        )),
        StandardContent::Attachment(value) => {
            xmtp_content_types::attachment::AttachmentCodec::encode(
                xmtp_content_types::attachment::Attachment {
                    filename: value.filename,
                    mime_type: value.mime_type,
                    content: value.content,
                },
            )
        }
        StandardContent::RemoteAttachment(value) => {
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::encode(value.into())
        }
        StandardContent::MultiRemoteAttachment(value) => {
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::encode(
                proto::MultiRemoteAttachment {
                    attachments: value.attachments.into_iter().map(Into::into).collect(),
                },
            )
        }
        StandardContent::TransactionReference(value) => {
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::encode(
                xmtp_content_types::transaction_reference::TransactionReference {
                    namespace: value.namespace,
                    network_id: value.network_id,
                    reference: value.reference,
                    metadata: value.metadata.map(|metadata| {
                        xmtp_content_types::transaction_reference::TransactionMetadata {
                            transaction_type: metadata.transaction_type,
                            currency: metadata.currency,
                            amount: metadata.amount,
                            decimals: metadata.decimals,
                            from_address: metadata.from_address,
                            to_address: metadata.to_address,
                        }
                    }),
                },
            )
        }
        StandardContent::WalletSendCalls(value) => {
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::encode(value.into())
        }
        StandardContent::Actions(value) => {
            xmtp_content_types::actions::ActionsCodec::encode(value.into())
        }
        StandardContent::Intent(value) => {
            xmtp_content_types::intent::IntentCodec::encode(value.try_into()?)
        }
        StandardContent::Reply {
            reference,
            reference_inbox_id,
            content,
        } => xmtp_content_types::reply::ReplyCodec::encode(xmtp_content_types::reply::Reply {
            reference: reference.0,
            reference_inbox_id: reference_inbox_id.map(|id| id.0),
            content: content.into(),
        }),
        StandardContent::GroupUpdated(value) => {
            xmtp_content_types::group_updated::GroupUpdatedCodec::encode(value.into())
        }
        StandardContent::DeleteMessage { message_id } => {
            xmtp_content_types::delete_message::DeleteMessageCodec::encode(proto::DeleteMessage {
                message_id: message_id.0,
            })
        }
        StandardContent::LeaveRequest(value) => {
            xmtp_content_types::leave_request::LeaveRequestCodec::encode(proto::LeaveRequest {
                authenticated_note: value.authenticated_note,
            })
        }
    }
    .map_err(codec_error)?;
    Ok(encoded.into())
}

#[xmtp_macro::sdk_export(pure)]
pub fn decode_standard(encoded: EncodedContent) -> Result<StandardContent, crate::XmtpError> {
    use xmtp_content_types::ContentCodec;
    let kind = [
        StandardContentKind::Text,
        StandardContentKind::Markdown,
        StandardContentKind::ReadReceipt,
        StandardContentKind::Reaction,
        StandardContentKind::Attachment,
        StandardContentKind::RemoteAttachment,
        StandardContentKind::MultiRemoteAttachment,
        StandardContentKind::TransactionReference,
        StandardContentKind::WalletSendCalls,
        StandardContentKind::Actions,
        StandardContentKind::Intent,
        StandardContentKind::Reply,
        StandardContentKind::GroupUpdated,
        StandardContentKind::DeleteMessage,
        StandardContentKind::LeaveRequest,
    ]
    .into_iter()
    .find(|kind| {
        let expected = standard_type(*kind);
        encoded.r#type.authority_id == expected.authority_id
            && encoded.r#type.type_id == expected.type_id
            && encoded.r#type.version_major == expected.version_major
    })
    .ok_or_else(|| crate::XmtpError::invalid("unsupported standard content type"))?;
    let encoded: ProtoEncodedContent = encoded.into();
    Ok(match kind {
        StandardContentKind::Text => StandardContent::Text(
            xmtp_content_types::text::TextCodec::decode(encoded).map_err(codec_error)?,
        ),
        StandardContentKind::Markdown => StandardContent::Markdown(
            xmtp_content_types::markdown::MarkdownCodec::decode(encoded).map_err(codec_error)?,
        ),
        StandardContentKind::ReadReceipt => {
            xmtp_content_types::read_receipt::ReadReceiptCodec::decode(encoded)
                .map_err(codec_error)?;
            StandardContent::ReadReceipt
        }
        StandardContentKind::Reaction => {
            let value = xmtp_content_types::reaction::ReactionCodec::decode(encoded)
                .map_err(codec_error)?;
            let reaction = Reaction::from_proto(value.clone());
            StandardContent::Reaction {
                reference: crate::MessageID::try_from(value.reference)?,
                reference_inbox_id: (!value.reference_inbox_id.is_empty())
                    .then_some(value.reference_inbox_id)
                    .map(crate::InboxID::try_from)
                    .transpose()?,
                reaction,
            }
        }
        StandardContentKind::Attachment => StandardContent::Attachment(
            xmtp_content_types::attachment::AttachmentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::RemoteAttachment => StandardContent::RemoteAttachment(
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::MultiRemoteAttachment => StandardContent::MultiRemoteAttachment(
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::decode(
                encoded,
            )
            .map_err(codec_error)?
            .into(),
        ),
        StandardContentKind::TransactionReference => StandardContent::TransactionReference(
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::WalletSendCalls => StandardContent::WalletSendCalls(
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::Actions => StandardContent::Actions(
            xmtp_content_types::actions::ActionsCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::Intent => StandardContent::Intent(
            xmtp_content_types::intent::IntentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::Reply => {
            let value =
                xmtp_content_types::reply::ReplyCodec::decode(encoded).map_err(codec_error)?;
            StandardContent::Reply {
                reference: crate::MessageID::try_from(value.reference)?,
                reference_inbox_id: value
                    .reference_inbox_id
                    .map(crate::InboxID::try_from)
                    .transpose()?,
                content: value.content.into(),
            }
        }
        StandardContentKind::GroupUpdated => StandardContent::GroupUpdated(
            xmtp_content_types::group_updated::GroupUpdatedCodec::decode(encoded)
                .map_err(codec_error)?
                .try_into()?,
        ),
        StandardContentKind::DeleteMessage => StandardContent::DeleteMessage {
            message_id: crate::MessageID::try_from(
                xmtp_content_types::delete_message::DeleteMessageCodec::decode(encoded)
                    .map_err(codec_error)?
                    .message_id,
            )?,
        },
        StandardContentKind::LeaveRequest => StandardContent::LeaveRequest(LeaveRequest {
            authenticated_note: xmtp_content_types::leave_request::LeaveRequestCodec::decode(
                encoded,
            )
            .map_err(codec_error)?
            .authenticated_note,
        }),
    })
}

#[xmtp_macro::sdk_export(pure)]
pub fn encode_text(text: String) -> Result<EncodedContent, crate::XmtpError> {
    encode_standard(StandardContent::Text(text))
}

impl From<EncodedContent> for ProtoEncodedContent {
    fn from(value: EncodedContent) -> Self {
        Self {
            r#type: Some(ProtoContentTypeId {
                authority_id: value.r#type.authority_id,
                type_id: value.r#type.type_id,
                version_major: value.r#type.version_major,
                version_minor: value.r#type.version_minor,
            }),
            parameters: value.parameters,
            fallback: value.fallback,
            compression: None,
            content: value.content,
        }
    }
}

impl From<ProtoEncodedContent> for EncodedContent {
    fn from(value: ProtoEncodedContent) -> Self {
        let value = if value.compression.is_some() {
            xmtp_content_types::compression::decompress(value.clone()).unwrap_or_else(|_| {
                ProtoEncodedContent {
                    content: Vec::new(),
                    compression: None,
                    ..value
                }
            })
        } else {
            value
        };
        let kind = value.r#type.unwrap_or_default();
        Self {
            r#type: ContentTypeId {
                authority_id: kind.authority_id,
                type_id: kind.type_id,
                version_major: kind.version_major,
                version_minor: kind.version_minor,
            },
            parameters: value.parameters,
            fallback: value.fallback,
            content: value.content,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum Compression {
    Deflate,
    Gzip,
}

impl From<Compression> for xmtp_proto::xmtp::mls::message_contents::Compression {
    fn from(value: Compression) -> Self {
        match value {
            Compression::Deflate => Self::Deflate,
            Compression::Gzip => Self::Gzip,
        }
    }
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SendOptions {
    #[uniffi(default = true)]
    pub should_push: bool,
    #[uniffi(default = false)]
    pub optimistic: bool,
    #[uniffi(default = None)]
    pub idempotency_key: Option<String>,
    #[uniffi(default = None)]
    pub compression: Option<Compression>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReactionAction {
    Added,
    Removed,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReactionSchema {
    Unicode,
    Shortcode,
    Custom,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Reaction {
    pub content: String,
    pub action: ReactionAction,
    pub schema: ReactionSchema,
}

impl Reaction {
    pub(crate) fn from_proto(
        value: xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2,
    ) -> Self {
        use xmtp_proto::xmtp::mls::message_contents::content_types::{
            ReactionAction as ProtoAction, ReactionSchema as ProtoSchema,
        };
        Self {
            content: value.content,
            action: if value.action == ProtoAction::Removed as i32 {
                ReactionAction::Removed
            } else {
                ReactionAction::Added
            },
            schema: match ProtoSchema::try_from(value.schema) {
                Ok(ProtoSchema::Shortcode) => ReactionSchema::Shortcode,
                Ok(ProtoSchema::Custom) => ReactionSchema::Custom,
                _ => ReactionSchema::Unicode,
            },
        }
    }

    pub(crate) fn into_proto(
        self,
        reference: crate::MessageID,
        reference_inbox_id: crate::InboxID,
    ) -> xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2 {
        use xmtp_proto::xmtp::mls::message_contents::content_types::{
            ReactionAction as ProtoAction, ReactionSchema as ProtoSchema, ReactionV2,
        };
        ReactionV2 {
            reference: reference.0,
            reference_inbox_id: reference_inbox_id.0,
            action: match self.action {
                ReactionAction::Added => ProtoAction::Added as i32,
                ReactionAction::Removed => ProtoAction::Removed as i32,
            },
            content: self.content,
            schema: match self.schema {
                ReactionSchema::Unicode => ProtoSchema::Unicode as i32,
                ReactionSchema::Shortcode => ProtoSchema::Shortcode as i32,
                ReactionSchema::Custom => ProtoSchema::Custom as i32,
            },
        }
    }
}

#[cfg(any(test, feature = "conformance"))]
pub(crate) mod pure_codec_tests {
    use super::*;
    use xmtp_content_types::ContentCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types as proto;

    pub(crate) fn standard_codec_samples()
    -> Result<Vec<(StandardContent, ProtoEncodedContent)>, Box<dyn std::error::Error>> {
        let text = xmtp_content_types::text::TextCodec::encode("hello".into())?;
        let remote = proto::RemoteAttachmentInfo {
            url: "https://example.test/file".into(),
            content_digest: "digest".into(),
            secret: vec![1; 32],
            salt: vec![2; 32],
            nonce: vec![3; 12],
            scheme: "https".into(),
            content_length: Some(10),
            filename: Some("file".into()),
        };
        let transaction = xmtp_content_types::transaction_reference::TransactionReference {
            namespace: None,
            network_id: "1".into(),
            reference: "0x1".into(),
            metadata: None,
        };
        let reaction = proto::ReactionV2 {
            reference: "a".repeat(64),
            reference_inbox_id: "inbox".into(),
            action: proto::ReactionAction::Added as i32,
            content: "👍".into(),
            schema: proto::ReactionSchema::Unicode as i32,
        };
        let reply = xmtp_content_types::reply::Reply {
            reference: "a".repeat(64),
            reference_inbox_id: Some("inbox".into()),
            content: text.clone(),
        };
        let update = xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
            initiated_by_inbox_id: "inbox".into(),
            ..Default::default()
        };
        let wallet = WalletSendCalls {
            version: "1".into(),
            chain_id: "0x1".into(),
            from: "0xsender".into(),
            calls: vec![],
            capabilities: None,
        };
        let actions = Actions {
            id: "actions".into(),
            description: "Choose".into(),
            actions: vec![Action {
                id: "one".into(),
                label: "One".into(),
                image_url: None,
                style: None,
                expires_at: None,
            }],
            expires_at: None,
        };
        let intent = Intent {
            id: "actions".into(),
            action_id: "one".into(),
            metadata_json: None,
        };
        let cases = vec![
            (StandardContent::Text("hello".into()), text),
            (
                StandardContent::Markdown("**hello**".into()),
                xmtp_content_types::markdown::MarkdownCodec::encode("**hello**".into())?,
            ),
            (
                StandardContent::ReadReceipt,
                xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
                    xmtp_content_types::read_receipt::ReadReceipt {},
                )?,
            ),
            (
                StandardContent::Reaction {
                    reference: crate::MessageID::try_from(reaction.reference.clone())?,
                    reference_inbox_id: Some(crate::InboxID::try_from(
                        reaction.reference_inbox_id.clone(),
                    )?),
                    reaction: Reaction::from_proto(reaction.clone()),
                },
                xmtp_content_types::reaction::ReactionCodec::encode(reaction)?,
            ),
            (
                StandardContent::Attachment(Attachment {
                    filename: None,
                    mime_type: "text/plain".into(),
                    content: b"file".to_vec(),
                }),
                xmtp_content_types::attachment::AttachmentCodec::encode(
                    xmtp_content_types::attachment::Attachment {
                        filename: None,
                        mime_type: "text/plain".into(),
                        content: b"file".to_vec(),
                    },
                )?,
            ),
            (
                StandardContent::RemoteAttachment(remote.clone().into()),
                xmtp_content_types::remote_attachment::RemoteAttachmentCodec::encode(
                    remote.clone(),
                )?,
            ),
            (
                StandardContent::MultiRemoteAttachment(MultiRemoteAttachment {
                    attachments: vec![remote.clone().into()],
                }),
                xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::encode(
                    proto::MultiRemoteAttachment {
                        attachments: vec![remote],
                    },
                )?,
            ),
            (
                StandardContent::TransactionReference(transaction.clone().into()),
                xmtp_content_types::transaction_reference::TransactionReferenceCodec::encode(
                    transaction,
                )?,
            ),
            (
                StandardContent::WalletSendCalls(wallet.clone()),
                xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::encode(wallet.into())?,
            ),
            (
                StandardContent::Actions(actions.clone()),
                xmtp_content_types::actions::ActionsCodec::encode(actions.into())?,
            ),
            (
                StandardContent::Intent(intent.clone()),
                xmtp_content_types::intent::IntentCodec::encode(intent.try_into()?)?,
            ),
            (
                StandardContent::Reply {
                    reference: crate::MessageID::try_from(reply.reference.clone())?,
                    reference_inbox_id: reply
                        .reference_inbox_id
                        .clone()
                        .map(crate::InboxID::try_from)
                        .transpose()?,
                    content: reply.content.clone().into(),
                },
                xmtp_content_types::reply::ReplyCodec::encode(reply)?,
            ),
            (
                StandardContent::GroupUpdated(update.clone().try_into()?),
                xmtp_content_types::group_updated::GroupUpdatedCodec::encode(update)?,
            ),
            (
                StandardContent::DeleteMessage {
                    message_id: crate::MessageID::try_from("a".repeat(64))?,
                },
                xmtp_content_types::delete_message::DeleteMessageCodec::encode(
                    proto::DeleteMessage {
                        message_id: "a".repeat(64),
                    },
                )?,
            ),
            (
                StandardContent::LeaveRequest(LeaveRequest {
                    authenticated_note: None,
                }),
                xmtp_content_types::leave_request::LeaveRequestCodec::encode(
                    proto::LeaveRequest {
                        authenticated_note: None,
                    },
                )?,
            ),
        ];
        Ok(cases)
    }

    #[cfg(test)]
    #[xmtp_common::test(unwrap_try = true)]
    fn standard_codec_bytes_match_the_core_send_codecs() {
        let cases = standard_codec_samples()?;
        assert_eq!(cases.len(), 15);
        for (value, core) in cases {
            let facade = encode_standard(value)?;
            let expected: EncodedContent = core.into();
            assert_eq!(facade.r#type.type_id, expected.r#type.type_id);
            assert_eq!(facade.content, expected.content);
            assert_eq!(facade.parameters, expected.parameters);
            assert_eq!(facade.fallback, expected.fallback);
            let round_trip = encode_standard(decode_standard(facade)?)?;
            assert_eq!(round_trip.content, expected.content);
        }
    }

    #[cfg(test)]
    #[xmtp_common::test(unwrap_try = true)]
    fn text_minor_version_decodes() {
        let mut encoded = encode_text("minor version".into())?;
        encoded.r#type.version_minor = 1;
        assert!(matches!(
            decode_standard(encoded)?,
            StandardContent::Text(value) if value == "minor version"
        ));
    }
}

/// Native host conformance compares each host codec with the core encoder.
#[cfg(feature = "conformance")]
#[derive(Clone, Debug, uniffi::Record)]
pub struct StandardCodecSample {
    pub value: StandardContent,
    pub expected: EncodedContent,
}

#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export(pure)]
pub fn sdk_conformance_standard_samples() -> Vec<StandardCodecSample> {
    pure_codec_tests::standard_codec_samples()
        .expect("fixed standard codec samples")
        .into_iter()
        .map(|(value, expected)| StandardCodecSample {
            value,
            expected: expected.into(),
        })
        .collect()
}
