use crate::{
  ErrorWrapper,
  conversation::Conversation,
  messages::decoded_message::DecodedMessage,
  messages::encoded_content::EncodedContent,
  messages::{ListMessagesOptions, Message},
};
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message as ProstMessage;
use std::{collections::HashMap, ops::Deref};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

#[napi(object)]
pub struct SendMessageOpts {
  pub should_push: bool,
  pub optimistic: Option<bool>,
}

impl From<SendMessageOpts> for xmtp_mls::groups::send_message_opts::SendMessageOpts {
  fn from(opts: SendMessageOpts) -> Self {
    xmtp_mls::groups::send_message_opts::SendMessageOpts {
      should_push: opts.should_push,
    }
  }
}

#[napi]
impl Conversation {
  #[napi]
  pub async fn send(
    &self,
    encoded_content: EncodedContent,
    opts: SendMessageOpts,
  ) -> Result<String> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.create_mls_group();

    let message_id = match opts.optimistic {
      Some(true) => group
        .send_message_optimistic(encoded_content.encode_to_vec().as_slice(), opts.into())
        .map_err(ErrorWrapper::from)?,
      _ => group
        .send_message(encoded_content.encode_to_vec().as_slice(), opts.into())
        .await
        .map_err(ErrorWrapper::from)?,
    };

    Ok(hex::encode(message_id))
  }

  #[napi]
  pub async fn publish_messages(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.publish_messages().await.map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn find_messages(&self, opts: Option<ListMessagesOptions>) -> Result<Vec<Message>> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let opts = MsgQueryArgs { ..opts.into() };
    let messages: Vec<Message> = group
      .find_messages(&opts)
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|msg| msg.into())
      .collect();

    Ok(messages)
  }

  #[napi]
  pub async fn count_messages(&self, opts: Option<ListMessagesOptions>) -> Result<i64> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let msg_args: MsgQueryArgs = opts.into();
    let count = group
      .count_messages(&msg_args)
      .map_err(ErrorWrapper::from)?;

    Ok(count)
  }

  #[napi]
  pub async fn process_streamed_group_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Vec<Message>> {
    let group = self.create_mls_group();
    let envelope_bytes: Vec<u8> = envelope_bytes.deref().to_vec();
    let message = group
      .process_streamed_group_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(message.into_iter().map(Into::into).collect())
  }

  #[napi]
  pub async fn find_enriched_messages(
    &self,
    opts: Option<ListMessagesOptions>,
  ) -> Result<Vec<DecodedMessage>> {
    let opts = opts.unwrap_or_default();
    let group = self.create_mls_group();
    let messages: Vec<DecodedMessage> = group
      .find_messages_v2(&opts.into())
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(|msg| msg.try_into())
      .collect::<Result<Vec<_>>>()?;

    Ok(messages)
  }

  #[napi]
  pub async fn get_last_read_times(&self) -> Result<HashMap<String, i64>> {
    let group = self.create_mls_group();
    let times = group.get_last_read_times().map_err(ErrorWrapper::from)?;
    Ok(times)
  }

  /// Prepare a message for later publishing.
  /// Stores the message locally without publishing. Returns the message ID.
  #[napi]
  pub fn prepare_message(
    &self,
    encoded_content: EncodedContent,
    should_push: bool,
  ) -> Result<String> {
    let encoded_content: XmtpEncodedContent = encoded_content.into();
    let group = self.create_mls_group();
    let message_id = group
      .prepare_message_for_later_publish(encoded_content.encode_to_vec().as_slice(), should_push)
      .map_err(ErrorWrapper::from)?;
    Ok(hex::encode(message_id))
  }

  /// Publish a previously prepared message by ID.
  #[napi]
  pub async fn publish_stored_message(&self, message_id: String) -> Result<()> {
    let group = self.create_mls_group();
    let message_id_bytes = hex::decode(&message_id).map_err(ErrorWrapper::from)?;
    group
      .publish_stored_message(&message_id_bytes)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }
}
