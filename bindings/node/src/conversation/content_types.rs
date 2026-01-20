use crate::ErrorWrapper;
use crate::{
  content_types::{
    actions::Actions, attachment::Attachment, intent::Intent,
    multi_remote_attachment::MultiRemoteAttachment, reaction::Reaction,
    remote_attachment::RemoteAttachment, reply::Reply, transaction_reference::TransactionReference,
    wallet_send_calls::WalletSendCalls,
  },
  conversation::Conversation,
  conversation::messages::SendMessageOpts,
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::{
  actions::ActionsCodec,
  attachment::AttachmentCodec,
  intent::IntentCodec,
  markdown::MarkdownCodec,
  multi_remote_attachment::MultiRemoteAttachmentCodec,
  reaction::ReactionCodec,
  read_receipt::{ReadReceipt, ReadReceiptCodec},
  remote_attachment::RemoteAttachmentCodec,
  reply::ReplyCodec,
  text::TextCodec,
  transaction_reference::TransactionReferenceCodec,
  wallet_send_calls::WalletSendCallsCodec,
};

#[napi]
impl Conversation {
  #[napi]
  pub async fn send_text(&self, text: String, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = TextCodec::encode(text).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: TextCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_markdown(&self, markdown: String, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = MarkdownCodec::encode(markdown).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: MarkdownCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reaction(
    &self,
    reaction: Reaction,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = ReactionCodec::encode(reaction.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReactionCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reply(&self, reply: Reply, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ReplyCodec::encode(reply.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReplyCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_read_receipt(&self, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ReadReceiptCodec::encode(ReadReceipt {}).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ReadReceiptCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_attachment(
    &self,
    attachment: Attachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = AttachmentCodec::encode(attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: AttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_remote_attachment(
    &self,
    remote_attachment: RemoteAttachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content =
      RemoteAttachmentCodec::encode(remote_attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: RemoteAttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_multi_remote_attachment(
    &self,
    multi_remote_attachment: MultiRemoteAttachment,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = MultiRemoteAttachmentCodec::encode(multi_remote_attachment.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: MultiRemoteAttachmentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_transaction_reference(
    &self,
    transaction_reference: TransactionReference,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let encoded_content = TransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: TransactionReferenceCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_wallet_send_calls(
    &self,
    wallet_send_calls: WalletSendCalls,
    optimistic: Option<bool>,
  ) -> Result<String> {
    let wsc = wallet_send_calls.try_into()?;
    let encoded_content = WalletSendCallsCodec::encode(wsc).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: WalletSendCallsCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_actions(&self, actions: Actions, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = ActionsCodec::encode(actions.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: ActionsCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_intent(&self, intent: Intent, optimistic: Option<bool>) -> Result<String> {
    let encoded_content = IntentCodec::encode(intent.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts {
      should_push: IntentCodec::should_push(),
      optimistic,
    };
    self.send(encoded_content.into(), opts).await
  }
}
