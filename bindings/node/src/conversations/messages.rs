use crate::ErrorWrapper;
use crate::conversation::Conversation;
use crate::conversations::Conversations;
use crate::messages::Message;
use crate::messages::decoded_message::DecodedMessage;
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;

#[napi]
impl Conversations {
  #[napi]
  pub fn find_message_by_id(&self, message_id: String) -> Result<Message> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let message = self
      .inner_client
      .message(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(Message::from(message))
  }

  #[napi]
  pub fn find_enriched_message_by_id(&self, message_id: String) -> Result<DecodedMessage> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let message = self
      .inner_client
      .message_v2(message_id)
      .map_err(ErrorWrapper::from)?;

    message.try_into()
  }

  #[napi]
  pub fn delete_message_by_id(&self, message_id: String) -> Result<u32> {
    let message_id = hex::decode(message_id).map_err(ErrorWrapper::from)?;

    let deleted_count = self
      .inner_client
      .delete_message(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(deleted_count as u32)
  }

  #[napi]
  pub async fn process_streamed_welcome_message(
    &self,
    envelope_bytes: Uint8Array,
  ) -> Result<Vec<Conversation>> {
    let envelope_bytes = envelope_bytes.deref().to_vec();
    let group = self
      .inner_client
      .process_streamed_welcome_message(envelope_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(group.into_iter().map(Into::into).collect())
  }
}
