use napi_derive::napi;
use xmtp_mls::messages::decoded_message::MessageBody;

use super::attachment::Attachment;
use super::group_updated::GroupUpdated;
use super::multi_remote_attachment::MultiRemoteAttachmentPayload;
use super::reaction::ReactionPayload;
use super::read_receipt::ReadReceipt;
use super::remote_attachment::RemoteAttachment;
use super::text::TextContent;
use super::transaction_reference::TransactionReference;
use super::wallet_send_calls::WalletSendCalls;
use crate::encoded_content::EncodedContent;

// Separate struct for reply content to prevent circular references
#[derive(Clone)]
#[napi(object)]
pub struct DecodedMessageBody {
  pub text_content: Option<TextContent>,
  pub reaction_content: Option<ReactionPayload>,
  pub attachment_content: Option<Attachment>,
  pub remote_attachment_content: Option<RemoteAttachment>,
  pub multi_remote_attachment_content: Option<MultiRemoteAttachmentPayload>,
  pub transaction_reference_content: Option<TransactionReference>,
  pub group_updated_content: Option<GroupUpdated>,
  pub read_receipt_content: Option<ReadReceipt>,
  pub wallet_send_calls_content: Option<WalletSendCalls>,
  pub delete_message_content: Option<bool>,
  pub deleted_message_content: Option<bool>,
  pub custom_content: Option<EncodedContent>,
}

impl From<MessageBody> for DecodedMessageBody {
  fn from(body: MessageBody) -> Self {
    let mut result = Self {
      text_content: None,
      reaction_content: None,
      attachment_content: None,
      remote_attachment_content: None,
      multi_remote_attachment_content: None,
      transaction_reference_content: None,
      group_updated_content: None,
      read_receipt_content: None,
      wallet_send_calls_content: None,
      delete_message_content: None,
      deleted_message_content: None,
      custom_content: None,
    };

    match body {
      MessageBody::Text(t) => result.text_content = Some(t.into()),
      MessageBody::Reaction(r) => result.reaction_content = Some(r.into()),
      MessageBody::Attachment(a) => result.attachment_content = Some(a.into()),
      MessageBody::RemoteAttachment(ra) => result.remote_attachment_content = Some(ra.into()),
      MessageBody::MultiRemoteAttachment(mra) => {
        result.multi_remote_attachment_content = Some(mra.into())
      }
      MessageBody::TransactionReference(tr) => {
        result.transaction_reference_content = Some(tr.into())
      }
      MessageBody::GroupUpdated(gu) => result.group_updated_content = Some(gu.into()),
      MessageBody::ReadReceipt(rr) => result.read_receipt_content = Some(rr.into()),
      MessageBody::WalletSendCalls(wsc) => result.wallet_send_calls_content = Some(wsc.into()),
      MessageBody::DeleteMessage(_) => result.delete_message_content = Some(true),
      MessageBody::DeletedMessage { .. } => result.deleted_message_content = Some(true),
      MessageBody::Custom(c) => result.custom_content = Some(c.into()),
      MessageBody::Reply(_) => {
        // This should not happen as we are converting from a reply's content
        // Return empty body rather than panicking
      }
    }

    result
  }
}
