use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use xmtp_mls::messages::decoded_message::MessageBody;

use super::{
  actions::Actions, attachment::Attachment, delete_message::DeletedMessage,
  group_updated::GroupUpdated, intent::Intent, leave_request::LeaveRequest,
  markdown::MarkdownContent, multi_remote_attachment::MultiRemoteAttachment, reaction::Reaction,
  read_receipt::ReadReceipt, remote_attachment::RemoteAttachment, reply::EnrichedReply,
  transaction_reference::TransactionReference, wallet_send_calls::WalletSendCalls,
};
use crate::encoded_content::EncodedContent;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DecodedMessageContent {
  Text { content: String },
  Markdown { content: MarkdownContent },
  Reply { content: Box<EnrichedReply> },
  Reaction { content: Reaction },
  Attachment { content: Attachment },
  RemoteAttachment { content: RemoteAttachment },
  MultiRemoteAttachment { content: MultiRemoteAttachment },
  TransactionReference { content: TransactionReference },
  GroupUpdated { content: GroupUpdated },
  ReadReceipt { content: ReadReceipt },
  LeaveRequest { content: LeaveRequest },
  WalletSendCalls { content: WalletSendCalls },
  Intent { content: Option<Intent> },
  Actions { content: Option<Actions> },
  DeleteMessage,
  DeletedMessage { content: DeletedMessage },
  Custom { content: EncodedContent },
}

impl TryFrom<MessageBody> for DecodedMessageContent {
  type Error = JsError;

  fn try_from(body: MessageBody) -> Result<Self, Self::Error> {
    match body {
      MessageBody::Text(t) => Ok(DecodedMessageContent::Text { content: t.content }),
      MessageBody::Markdown(m) => Ok(DecodedMessageContent::Markdown { content: m.into() }),
      MessageBody::Reply(r) => Ok(DecodedMessageContent::Reply {
        content: Box::new(r.try_into()?),
      }),
      MessageBody::Reaction(r) => Ok(DecodedMessageContent::Reaction { content: r.into() }),
      MessageBody::Attachment(a) => Ok(DecodedMessageContent::Attachment { content: a.into() }),
      MessageBody::RemoteAttachment(ra) => Ok(DecodedMessageContent::RemoteAttachment {
        content: ra.try_into()?,
      }),
      MessageBody::MultiRemoteAttachment(mra) => Ok(DecodedMessageContent::MultiRemoteAttachment {
        content: mra.into(),
      }),
      MessageBody::TransactionReference(tr) => {
        Ok(DecodedMessageContent::TransactionReference { content: tr.into() })
      }
      MessageBody::GroupUpdated(gu) => {
        Ok(DecodedMessageContent::GroupUpdated { content: gu.into() })
      }
      MessageBody::ReadReceipt(rr) => Ok(DecodedMessageContent::ReadReceipt { content: rr.into() }),
      MessageBody::LeaveRequest(lr) => {
        Ok(DecodedMessageContent::LeaveRequest { content: lr.into() })
      }
      MessageBody::WalletSendCalls(wsc) => Ok(DecodedMessageContent::WalletSendCalls {
        content: wsc.try_into()?,
      }),
      MessageBody::Intent(i) => Ok(DecodedMessageContent::Intent {
        content: i.map(Into::into),
      }),
      MessageBody::Actions(a) => Ok(DecodedMessageContent::Actions {
        content: a.map(|a| a.try_into()).transpose()?,
      }),
      MessageBody::DeleteMessage(_) => {
        // DeleteMessage itself shouldn't appear in decoded messages for WASM
        // It's only used internally for deletion processing
        Ok(DecodedMessageContent::DeleteMessage)
      }
      MessageBody::DeletedMessage { deleted_by } => Ok(DecodedMessageContent::DeletedMessage {
        content: deleted_by.into(),
      }),
      MessageBody::Custom(c) => Ok(DecodedMessageContent::Custom { content: c.into() }),
    }
  }
}
