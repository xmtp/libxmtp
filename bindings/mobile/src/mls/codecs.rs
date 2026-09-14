//! Content-type encode and decode helpers.

use super::*;

use crate::message::{FfiActions, FfiIntent, FfiReactionPayload};
use crate::{FfiError, FfiGroupUpdated, FfiReply, FfiWalletSendCalls};
use prost::Message;
use std::convert::TryInto;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::actions::{Actions, ActionsCodec};
use xmtp_content_types::attachment::Attachment;
use xmtp_content_types::attachment::AttachmentCodec;
use xmtp_content_types::delete_message::DeleteMessageCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_content_types::intent::{Intent, IntentCodec};
use xmtp_content_types::leave_request::LeaveRequestCodec;
use xmtp_content_types::markdown::MarkdownCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_content_types::read_receipt::ReadReceipt;
use xmtp_content_types::read_receipt::ReadReceiptCodec;
use xmtp_content_types::remote_attachment::RemoteAttachment;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec;
use xmtp_content_types::reply::Reply;
use xmtp_content_types::reply::ReplyCodec;
use xmtp_content_types::text::TextCodec;
use xmtp_content_types::transaction_reference::TransactionReference;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;
use xmtp_content_types::wallet_send_calls::WalletSendCallsCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
use xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest;
use xmtp_proto::xmtp::mls::message_contents::content_types::{MultiRemoteAttachment, ReactionV2};

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_reaction(reaction: FfiReactionPayload) -> Result<Vec<u8>, FfiError> {
    // Convert FfiReaction to Reaction
    let reaction: ReactionV2 = reaction.into();

    // Use ReactionCodec to encode the reaction
    let encoded = ReactionCodec::encode(reaction).map_err(|e| FfiError::generic(e.to_string()))?;

    // Encode the EncodedContent to bytes
    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_reaction(bytes: Vec<u8>) -> Result<FfiReactionPayload, FfiError> {
    // Decode bytes into EncodedContent
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    // Use ReactionCodec to decode into Reaction and convert to FfiReaction
    ReactionCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// RemoteAttachmentInfo and MultiRemoteAttachment FFI structures - using types from message module

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_multi_remote_attachment(
    ffi_multi_remote_attachment: FfiMultiRemoteAttachment,
) -> Result<Vec<u8>, FfiError> {
    // Convert FfiMultiRemoteAttachment to MultiRemoteAttachment
    let multi_remote_attachment: MultiRemoteAttachment = ffi_multi_remote_attachment.into();

    // Use MultiRemoteAttachmentCodec to encode the reaction
    let encoded = MultiRemoteAttachmentCodec::encode(multi_remote_attachment)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    // Encode the EncodedContent to bytes
    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_multi_remote_attachment(
    bytes: Vec<u8>,
) -> Result<FfiMultiRemoteAttachment, FfiError> {
    // Decode bytes into EncodedContent
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    // Use MultiRemoteAttachmentCodec to decode into MultiRemoteAttachment and convert to FfiMultiRemoteAttachment
    MultiRemoteAttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// TransactionReference FFI structures - using types from message module

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_transaction_reference(
    reference: FfiTransactionReference,
) -> Result<Vec<u8>, FfiError> {
    let reference: TransactionReference = reference.into();

    let encoded = TransactionReferenceCodec::encode(reference)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_transaction_reference(bytes: Vec<u8>) -> Result<FfiTransactionReference, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    TransactionReferenceCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// Attachment FFI structures - using FfiAttachment from message module

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_attachment(attachment: FfiAttachment) -> Result<Vec<u8>, FfiError> {
    let attachment: Attachment = attachment.into();

    let encoded =
        AttachmentCodec::encode(attachment).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_attachment(bytes: Vec<u8>) -> Result<FfiAttachment, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    AttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_reply(reply: FfiReply) -> Result<Vec<u8>, FfiError> {
    let reply: Reply = reply.into();

    let encoded = ReplyCodec::encode(reply).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_reply(bytes: Vec<u8>) -> Result<FfiReply, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    ReplyCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// ReadReceipt FFI structures - using FfiReadReceipt from message module

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_read_receipt(read_receipt: FfiReadReceipt) -> Result<Vec<u8>, FfiError> {
    let read_receipt: ReadReceipt = read_receipt.into();

    let encoded =
        ReadReceiptCodec::encode(read_receipt).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_read_receipt(bytes: Vec<u8>) -> Result<FfiReadReceipt, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    ReadReceiptCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// RemoteAttachment FFI structures - using FfiRemoteAttachment from message module

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_remote_attachment(
    remote_attachment: FfiRemoteAttachment,
) -> Result<Vec<u8>, FfiError> {
    let remote_attachment: RemoteAttachment = remote_attachment.into();

    let encoded = RemoteAttachmentCodec::encode(remote_attachment)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_remote_attachment(bytes: Vec<u8>) -> Result<FfiRemoteAttachment, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    RemoteAttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// Intent FFI encode/decode functions

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_intent(intent: FfiIntent) -> Result<Vec<u8>, FfiError> {
    let intent: Intent = intent.try_into()?;

    let encoded = IntentCodec::encode(intent).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_intent(bytes: Vec<u8>) -> Result<FfiIntent, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    let intent =
        IntentCodec::decode(encoded_content).map_err(|e| FfiError::generic(e.to_string()))?;

    intent.try_into().map_err(Into::into)
}

// Actions FFI encode/decode functions

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_actions(actions: FfiActions) -> Result<Vec<u8>, FfiError> {
    let actions: Actions = actions.into();

    let encoded = ActionsCodec::encode(actions).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_actions(bytes: Vec<u8>) -> Result<FfiActions, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    let actions =
        ActionsCodec::decode(encoded_content).map_err(|e| FfiError::generic(e.to_string()))?;

    actions.try_into().map_err(Into::into)
}

// LeaveRequest FFI encode function
#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_leave_request(request: FfiLeaveRequest) -> Result<Vec<u8>, FfiError> {
    let leave_request: LeaveRequest = request.into();

    let encoded =
        LeaveRequestCodec::encode(leave_request).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

// LeaveRequest FFI decode function
#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_leave_request(bytes: Vec<u8>) -> Result<FfiLeaveRequest, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    LeaveRequestCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

// DeleteMessage FFI encode function
#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_delete_message(request: FfiDeleteMessage) -> Result<Vec<u8>, FfiError> {
    let delete_message: DeleteMessage = request.into();

    let encoded =
        DeleteMessageCodec::encode(delete_message).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

// DeleteMessage FFI decode function
#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_delete_message(bytes: Vec<u8>) -> Result<FfiDeleteMessage, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    DeleteMessageCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_group_updated(bytes: Vec<u8>) -> Result<FfiGroupUpdated, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    GroupUpdatedCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_text(text: String) -> Result<Vec<u8>, FfiError> {
    let encoded = TextCodec::encode(text).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_text(bytes: Vec<u8>) -> Result<String, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    TextCodec::decode(encoded_content).map_err(|e| FfiError::generic(e.to_string()))
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_markdown(text: String) -> Result<Vec<u8>, FfiError> {
    let encoded = MarkdownCodec::encode(text).map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_markdown(bytes: Vec<u8>) -> Result<String, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    MarkdownCodec::decode(encoded_content).map_err(|e| FfiError::generic(e.to_string()))
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn encode_wallet_send_calls(
    wallet_send_calls: FfiWalletSendCalls,
) -> Result<Vec<u8>, FfiError> {
    let encoded = WalletSendCallsCodec::encode(wallet_send_calls.into())
        .map_err(|e| FfiError::generic(e.to_string()))?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| FfiError::generic(e.to_string()))?;

    Ok(buf)
}

#[uniffi::export]
#[tracing::instrument(skip_all)]
pub fn decode_wallet_send_calls(bytes: Vec<u8>) -> Result<FfiWalletSendCalls, FfiError> {
    let encoded_content =
        EncodedContent::decode(bytes.as_slice()).map_err(|e| FfiError::generic(e.to_string()))?;

    WalletSendCallsCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| FfiError::generic(e.to_string()))
}
