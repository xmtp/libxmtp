use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_mls::messages::decoded_message::MessageBody;

use super::{
  actions::Actions, attachment::Attachment, group_updated::GroupUpdated, intent::Intent,
  multi_remote_attachment::MultiRemoteAttachment, reaction::ReactionPayload,
  read_receipt::ReadReceipt, remote_attachment::RemoteAttachment, reply::EnrichedReply,
  text::TextContent, transaction_reference::TransactionReference,
  wallet_send_calls::WalletSendCalls,
};
use crate::encoded_content::EncodedContent;

#[wasm_bindgen]
pub enum PayloadType {
  Text,
  Reply,
  Reaction,
  Attachment,
  RemoteAttachment,
  MultiRemoteAttachment,
  TransactionReference,
  GroupUpdated,
  ReadReceipt,
  WalletSendCalls,
  Intent,
  Actions,
  Custom,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct DecodedMessageContent {
  payload: MessageBody,
}

#[wasm_bindgen]
impl DecodedMessageContent {
  #[wasm_bindgen(getter, js_name = payloadType)]
  pub fn payload_type(&self) -> PayloadType {
    match &self.payload {
      MessageBody::Text(_) => PayloadType::Text,
      MessageBody::Reply(_) => PayloadType::Reply,
      MessageBody::Reaction(_) => PayloadType::Reaction,
      MessageBody::Attachment(_) => PayloadType::Attachment,
      MessageBody::RemoteAttachment(_) => PayloadType::RemoteAttachment,
      MessageBody::MultiRemoteAttachment(_) => PayloadType::MultiRemoteAttachment,
      MessageBody::TransactionReference(_) => PayloadType::TransactionReference,
      MessageBody::GroupUpdated(_) => PayloadType::GroupUpdated,
      MessageBody::ReadReceipt(_) => PayloadType::ReadReceipt,
      MessageBody::WalletSendCalls(_) => PayloadType::WalletSendCalls,
      MessageBody::Intent(_) => PayloadType::Intent,
      MessageBody::Actions(_) => PayloadType::Actions,
      MessageBody::Custom(_) => PayloadType::Custom,
    }
  }

  #[wasm_bindgen(js_name = asText)]
  pub fn as_text(&self) -> Option<TextContent> {
    match &self.payload {
      MessageBody::Text(t) => Some(t.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asReply)]
  pub fn as_reply(&self) -> Option<EnrichedReply> {
    match &self.payload {
      MessageBody::Reply(r) => Some(r.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asReaction)]
  pub fn as_reaction(&self) -> Option<ReactionPayload> {
    match &self.payload {
      MessageBody::Reaction(r) => Some(r.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asAttachment)]
  pub fn as_attachment(&self) -> Option<Attachment> {
    match &self.payload {
      MessageBody::Attachment(a) => Some(a.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asRemoteAttachment)]
  pub fn as_remote_attachment(&self) -> Option<RemoteAttachment> {
    match &self.payload {
      MessageBody::RemoteAttachment(ra) => match ra.clone().try_into() {
        Ok(ra) => Some(ra),
        Err(e) => {
          tracing::error!("Failed to convert RemoteAttachment: {:?}", e);
          None
        }
      },
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asMultiRemoteAttachment)]
  pub fn as_multi_remote_attachment(&self) -> Option<MultiRemoteAttachment> {
    match &self.payload {
      MessageBody::MultiRemoteAttachment(mra) => Some(mra.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asTransactionReference)]
  pub fn as_transaction_reference(&self) -> Option<TransactionReference> {
    match &self.payload {
      MessageBody::TransactionReference(tr) => Some(tr.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asGroupUpdated)]
  pub fn as_group_updated(&self) -> Option<GroupUpdated> {
    match &self.payload {
      MessageBody::GroupUpdated(gu) => Some(gu.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asReadReceipt)]
  pub fn as_read_receipt(&self) -> Option<ReadReceipt> {
    match &self.payload {
      MessageBody::ReadReceipt(rr) => Some(rr.clone().into()),
      _ => None,
    }
  }

  #[wasm_bindgen(js_name = asWalletSendCalls)]
  pub fn as_wallet_send_calls(&self) -> Result<JsValue, JsValue> {
    match &self.payload {
      MessageBody::WalletSendCalls(wsc) => {
        let converted: WalletSendCalls = wsc.clone().into();
        serde_wasm_bindgen::to_value(&converted)
          .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
      }
      _ => Ok(JsValue::NULL),
    }
  }

  #[wasm_bindgen(js_name = asIntent)]
  pub fn as_intent(&self) -> Result<Option<Intent>, JsError> {
    match &self.payload {
      MessageBody::Intent(Some(intent)) => Ok(Some(intent.clone().try_into()?)),
      _ => Ok(None),
    }
  }

  #[wasm_bindgen(js_name = asActions)]
  pub fn as_actions(&self) -> Result<Option<Actions>, JsError> {
    match &self.payload {
      MessageBody::Actions(Some(actions)) => Ok(Some(actions.clone().try_into()?)),
      _ => Ok(None),
    }
  }

  #[wasm_bindgen(js_name = asCustom)]
  pub fn as_custom(&self) -> Option<EncodedContent> {
    match &self.payload {
      MessageBody::Custom(c) => Some(c.clone().into()),
      _ => None,
    }
  }
}

impl From<MessageBody> for DecodedMessageContent {
  fn from(body: MessageBody) -> Self {
    DecodedMessageContent { payload: body }
  }
}
