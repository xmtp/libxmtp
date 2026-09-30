use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use xmtp_mls::messages::decoded_message::{
  ContentDecodeFailureKind as XmtpContentDecodeFailureKind, MessageBody,
  UndecodableContent as XmtpUndecodableContent,
};

use super::{
  actions::Actions, attachment::Attachment, deleted_message::DeletedMessage,
  group_updated::GroupUpdated, intent::Intent, leave_request::LeaveRequest,
  multi_remote_attachment::MultiRemoteAttachment, reaction::Reaction, read_receipt::ReadReceipt,
  remote_attachment::RemoteAttachment, reply::EnrichedReply,
  transaction_reference::TransactionReference, wallet_send_calls::WalletSendCalls,
};
use crate::encoded_content::{ContentTypeId, EncodedContent};

/// Why received content could not be decoded.
#[derive(Clone, Serialize, Deserialize, Tsify, PartialEq)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum ContentDecodeFailureKind {
  MalformedEnvelope,
  CodecDecodeFailed,
}

/// Received content the client could not decode, kept with its exact bytes.
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct UndecodableContent {
  /// The exact received bytes of the serialized envelope.
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub raw_bytes: Vec<u8>,
  /// The received identifier, when the envelope parsed and carried one. It can be incomplete.
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub content_type: Option<ContentTypeId>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub fallback: Option<String>,
  pub failure_kind: ContentDecodeFailureKind,
  pub failure_message: String,
}

impl From<XmtpUndecodableContent> for UndecodableContent {
  fn from(content: XmtpUndecodableContent) -> Self {
    let envelope = content.encoded;
    Self {
      raw_bytes: content.raw_bytes,
      content_type: envelope
        .as_ref()
        .and_then(|envelope| envelope.r#type.clone())
        .map(Into::into),
      fallback: envelope.and_then(|envelope| envelope.fallback),
      failure_kind: match content.failure.kind {
        XmtpContentDecodeFailureKind::MalformedEnvelope => {
          ContentDecodeFailureKind::MalformedEnvelope
        }
        XmtpContentDecodeFailureKind::CodecDecodeFailed => {
          ContentDecodeFailureKind::CodecDecodeFailed
        }
      },
      failure_message: content.failure.message,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DecodedMessageContent {
  Actions { content: Option<Actions> },
  Attachment { content: Attachment },
  Custom { content: EncodedContent },
  GroupUpdated { content: GroupUpdated },
  Intent { content: Option<Intent> },
  LeaveRequest { content: LeaveRequest },
  Markdown { content: String },
  MultiRemoteAttachment { content: MultiRemoteAttachment },
  Reaction { content: Reaction },
  ReadReceipt { content: ReadReceipt },
  RemoteAttachment { content: RemoteAttachment },
  Reply { content: Box<EnrichedReply> },
  Text { content: String },
  TransactionReference { content: TransactionReference },
  WalletSendCalls { content: WalletSendCalls },
  DeletedMessage { content: DeletedMessage },
  Undecodable { content: UndecodableContent },
}

impl TryFrom<MessageBody> for DecodedMessageContent {
  type Error = JsError;

  fn try_from(body: MessageBody) -> Result<Self, Self::Error> {
    match body {
      MessageBody::Actions(a) => Ok(DecodedMessageContent::Actions {
        content: a.map(|a| a.try_into()).transpose()?,
      }),
      MessageBody::Attachment(a) => Ok(DecodedMessageContent::Attachment { content: a.into() }),
      MessageBody::Custom(c) => Ok(DecodedMessageContent::Custom {
        content: c.encoded.into(),
      }),
      MessageBody::Undecodable(u) => Ok(DecodedMessageContent::Undecodable { content: u.into() }),
      MessageBody::GroupUpdated(gu) => {
        Ok(DecodedMessageContent::GroupUpdated { content: gu.into() })
      }
      MessageBody::Intent(i) => Ok(DecodedMessageContent::Intent {
        content: i.map(Into::into),
      }),
      MessageBody::LeaveRequest(lr) => {
        Ok(DecodedMessageContent::LeaveRequest { content: lr.into() })
      }
      MessageBody::Markdown(m) => Ok(DecodedMessageContent::Markdown { content: m.content }),
      MessageBody::MultiRemoteAttachment(mra) => Ok(DecodedMessageContent::MultiRemoteAttachment {
        content: mra.into(),
      }),
      MessageBody::Reaction(r) => Ok(DecodedMessageContent::Reaction { content: r.into() }),
      MessageBody::ReadReceipt(rr) => Ok(DecodedMessageContent::ReadReceipt { content: rr.into() }),
      MessageBody::RemoteAttachment(ra) => {
        Ok(DecodedMessageContent::RemoteAttachment { content: ra.into() })
      }
      MessageBody::Reply(r) => Ok(DecodedMessageContent::Reply {
        content: Box::new(r.try_into()?),
      }),
      MessageBody::Text(t) => Ok(DecodedMessageContent::Text { content: t.content }),
      MessageBody::TransactionReference(tr) => {
        Ok(DecodedMessageContent::TransactionReference { content: tr.into() })
      }
      MessageBody::WalletSendCalls(wsc) => Ok(DecodedMessageContent::WalletSendCalls {
        content: wsc.try_into()?,
      }),
      MessageBody::DeletedMessage { deleted_by } => Ok(DecodedMessageContent::DeletedMessage {
        content: deleted_by.into(),
      }),
    }
  }
}
