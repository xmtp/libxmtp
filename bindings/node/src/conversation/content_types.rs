use crate::ErrorWrapper;
use crate::{
  content_types::{
    actions::Actions, attachment::Attachment, intent::Intent,
    multi_remote_attachment::MultiRemoteAttachment, reaction::Reaction,
    remote_attachment::RemoteAttachment, reply::Reply, transaction_reference::TransactionReference,
    wallet_send_calls::WalletSendCalls,
  },
  conversation::Conversation,
  conversation::messages::{SendMessageOpts, SendOpts},
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

impl SendMessageOpts {
  /// Build full send opts from a codec-derived `should_push` and the caller's
  /// convenience `SendOpts` bag.
  fn from_send_opts(should_push: bool, opts: Option<SendOpts>) -> Self {
    let opts = opts.unwrap_or_default();
    SendMessageOpts {
      should_push,
      optimistic: opts.optimistic,
      idempotency_key: opts.idempotency_key,
    }
  }
}

#[napi]
impl Conversation {
  #[napi]
  pub async fn send_text(&self, text: String, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = TextCodec::encode(text).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(TextCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_markdown(&self, markdown: String, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = MarkdownCodec::encode(markdown).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(MarkdownCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reaction(&self, reaction: Reaction, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = ReactionCodec::encode(reaction.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(ReactionCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_reply(&self, reply: Reply, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = ReplyCodec::encode(reply.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(ReplyCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_read_receipt(&self, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = ReadReceiptCodec::encode(ReadReceipt {}).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(ReadReceiptCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_attachment(
    &self,
    attachment: Attachment,
    opts: Option<SendOpts>,
  ) -> Result<String> {
    let encoded_content = AttachmentCodec::encode(attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(AttachmentCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_remote_attachment(
    &self,
    remote_attachment: RemoteAttachment,
    opts: Option<SendOpts>,
  ) -> Result<String> {
    let encoded_content =
      RemoteAttachmentCodec::encode(remote_attachment.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(RemoteAttachmentCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_multi_remote_attachment(
    &self,
    multi_remote_attachment: MultiRemoteAttachment,
    opts: Option<SendOpts>,
  ) -> Result<String> {
    let encoded_content = MultiRemoteAttachmentCodec::encode(multi_remote_attachment.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(MultiRemoteAttachmentCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_transaction_reference(
    &self,
    transaction_reference: TransactionReference,
    opts: Option<SendOpts>,
  ) -> Result<String> {
    let encoded_content = TransactionReferenceCodec::encode(transaction_reference.into())
      .map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(TransactionReferenceCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_wallet_send_calls(
    &self,
    wallet_send_calls: WalletSendCalls,
    opts: Option<SendOpts>,
  ) -> Result<String> {
    let wsc = wallet_send_calls.try_into()?;
    let encoded_content = WalletSendCallsCodec::encode(wsc).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(WalletSendCallsCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_actions(&self, actions: Actions, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = ActionsCodec::encode(actions.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(ActionsCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }

  #[napi]
  pub async fn send_intent(&self, intent: Intent, opts: Option<SendOpts>) -> Result<String> {
    let encoded_content = IntentCodec::encode(intent.into()).map_err(ErrorWrapper::from)?;
    let opts = SendMessageOpts::from_send_opts(IntentCodec::should_push(), opts);
    self.send(encoded_content.into(), opts).await
  }
}
