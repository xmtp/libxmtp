use napi::bindgen_prelude::*;
use napi_derive::napi;
use xmtp_mls::messages::decoded_message::MessageBody;

use super::actions::Actions;
use super::attachment::Attachment;
use super::deleted_message::DeletedMessage;
use super::group_updated::GroupUpdated;
use super::intent::Intent;
use super::leave_request::LeaveRequest;
use super::multi_remote_attachment::MultiRemoteAttachment;
use super::reaction::Reaction;
use super::read_receipt::ReadReceipt;
use super::remote_attachment::RemoteAttachment;
use super::reply::EnrichedReply;
use super::transaction_reference::TransactionReference;
use super::wallet_send_calls::WalletSendCalls;
use crate::encoded_content::EncodedContent;

#[napi(string_enum)]
#[derive(Clone, PartialEq)]
pub enum DecodedMessageContentType {
  Actions,
  Attachment,
  Custom,
  GroupUpdated,
  Intent,
  LeaveRequest,
  Markdown,
  MultiRemoteAttachment,
  Reaction,
  ReadReceipt,
  RemoteAttachment,
  Reply,
  Text,
  TransactionReference,
  WalletSendCalls,
  DeletedMessage,
}

#[derive(Clone)]
pub enum DecodedMessageContentInner {
  Actions(Option<Actions>),
  Attachment(Attachment),
  Custom(EncodedContent),
  GroupUpdated(GroupUpdated),
  Intent(Option<Intent>),
  LeaveRequest(LeaveRequest),
  Markdown(String),
  MultiRemoteAttachment(MultiRemoteAttachment),
  Reaction(Reaction),
  ReadReceipt(ReadReceipt),
  RemoteAttachment(RemoteAttachment),
  Reply(EnrichedReply),
  Text(String),
  TransactionReference(TransactionReference),
  WalletSendCalls(WalletSendCalls),
    DeletedMessage(DeletedMessage),
}

#[derive(Clone)]
#[napi]
pub struct DecodedMessageContent {
  inner: DecodedMessageContentInner,
}

#[napi]
impl DecodedMessageContent {
  #[napi(getter, js_name = "type")]
  pub fn content_type(&self) -> DecodedMessageContentType {
    match &self.inner {
      DecodedMessageContentInner::Actions(_) => DecodedMessageContentType::Actions,
      DecodedMessageContentInner::Attachment(_) => DecodedMessageContentType::Attachment,
      DecodedMessageContentInner::Custom(_) => DecodedMessageContentType::Custom,
      DecodedMessageContentInner::GroupUpdated(_) => DecodedMessageContentType::GroupUpdated,
      DecodedMessageContentInner::Intent(_) => DecodedMessageContentType::Intent,
      DecodedMessageContentInner::LeaveRequest(_) => DecodedMessageContentType::LeaveRequest,
      DecodedMessageContentInner::Markdown(_) => DecodedMessageContentType::Markdown,
      DecodedMessageContentInner::MultiRemoteAttachment(_) => {
        DecodedMessageContentType::MultiRemoteAttachment
      }
      DecodedMessageContentInner::Reaction(_) => DecodedMessageContentType::Reaction,
      DecodedMessageContentInner::ReadReceipt(_) => DecodedMessageContentType::ReadReceipt,
      DecodedMessageContentInner::RemoteAttachment(_) => {
        DecodedMessageContentType::RemoteAttachment
      }
      DecodedMessageContentInner::Reply(_) => DecodedMessageContentType::Reply,
      DecodedMessageContentInner::Text(_) => DecodedMessageContentType::Text,
      DecodedMessageContentInner::TransactionReference(_) => {
        DecodedMessageContentType::TransactionReference
      }
      DecodedMessageContentInner::WalletSendCalls(_) => DecodedMessageContentType::WalletSendCalls,
      DecodedMessageContentInner::DeletedMessage(_) => DecodedMessageContentType::DeletedMessage,
    }
  }

  #[napi(getter)]
  pub fn text(&self) -> Option<String> {
    match &self.inner {
      DecodedMessageContentInner::Text(t) => Some(t.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn markdown(&self) -> Option<String> {
    match &self.inner {
      DecodedMessageContentInner::Markdown(m) => Some(m.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn reply(&self) -> Option<EnrichedReply> {
    match &self.inner {
      DecodedMessageContentInner::Reply(r) => Some(r.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn reaction(&self) -> Option<Reaction> {
    match &self.inner {
      DecodedMessageContentInner::Reaction(r) => Some(r.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn attachment(&self) -> Option<Attachment> {
    match &self.inner {
      DecodedMessageContentInner::Attachment(a) => Some(a.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn remote_attachment(&self) -> Option<RemoteAttachment> {
    match &self.inner {
      DecodedMessageContentInner::RemoteAttachment(ra) => Some(ra.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn multi_remote_attachment(&self) -> Option<MultiRemoteAttachment> {
    match &self.inner {
      DecodedMessageContentInner::MultiRemoteAttachment(mra) => Some(mra.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn transaction_reference(&self) -> Option<TransactionReference> {
    match &self.inner {
      DecodedMessageContentInner::TransactionReference(tr) => Some(tr.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn group_updated(&self) -> Option<GroupUpdated> {
    match &self.inner {
      DecodedMessageContentInner::GroupUpdated(gu) => Some(gu.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn read_receipt(&self) -> Option<ReadReceipt> {
    match &self.inner {
      DecodedMessageContentInner::ReadReceipt(rr) => Some(rr.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn leave_request(&self) -> Option<LeaveRequest> {
    match &self.inner {
      DecodedMessageContentInner::LeaveRequest(lr) => Some(lr.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn wallet_send_calls(&self) -> Option<WalletSendCalls> {
    match &self.inner {
      DecodedMessageContentInner::WalletSendCalls(wsc) => Some(wsc.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn intent(&self) -> Option<Intent> {
    match &self.inner {
      DecodedMessageContentInner::Intent(i) => i.clone(),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn actions(&self) -> Option<Actions> {
    match &self.inner {
      DecodedMessageContentInner::Actions(a) => a.clone(),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn deleted_message(&self) -> Option<DeletedMessage> {
    match &self.inner {
      DecodedMessageContentInner::DeletedMessage(dm) => Some(dm.clone()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn custom(&self) -> Option<EncodedContent> {
    match &self.inner {
      DecodedMessageContentInner::Custom(c) => Some(c.clone()),
      _ => None,
    }
  }
}

impl TryFrom<MessageBody> for DecodedMessageContent {
  type Error = Error;

  fn try_from(body: MessageBody) -> Result<Self> {
    let inner = match body {
      MessageBody::Text(t) => DecodedMessageContentInner::Text(t.content),
      MessageBody::Markdown(m) => DecodedMessageContentInner::Markdown(m.content),
      MessageBody::Reply(r) => DecodedMessageContentInner::Reply(r.into()),
      MessageBody::Reaction(r) => DecodedMessageContentInner::Reaction(r.into()),
      MessageBody::Attachment(a) => DecodedMessageContentInner::Attachment(a.into()),
      MessageBody::RemoteAttachment(ra) => {
        DecodedMessageContentInner::RemoteAttachment(ra.try_into()?)
      }
      MessageBody::MultiRemoteAttachment(mra) => {
        DecodedMessageContentInner::MultiRemoteAttachment(mra.into())
      }
      MessageBody::TransactionReference(tr) => {
        DecodedMessageContentInner::TransactionReference(tr.into())
      }
      MessageBody::GroupUpdated(gu) => DecodedMessageContentInner::GroupUpdated(gu.into()),
      MessageBody::ReadReceipt(rr) => DecodedMessageContentInner::ReadReceipt(rr.into()),
      MessageBody::LeaveRequest(lr) => DecodedMessageContentInner::LeaveRequest(lr.into()),
      MessageBody::WalletSendCalls(wsc) => {
        DecodedMessageContentInner::WalletSendCalls(wsc.try_into()?)
      }
      MessageBody::Intent(i) => DecodedMessageContentInner::Intent(i.map(Into::into)),
      MessageBody::Actions(a) => {
        let actions = match a {
          Some(actions) => Some(actions.try_into()?),
          None => None,
        };
        DecodedMessageContentInner::Actions(actions)
      }
      MessageBody::DeletedMessage { deleted_by } => {
        DecodedMessageContentInner::DeletedMessage(deleted_by.into())
      }
      MessageBody::Custom(c) => DecodedMessageContentInner::Custom(c.into()),
    };

    Ok(Self { inner })
  }
}
