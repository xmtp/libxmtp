use crate::{ErrorWrapper, conversation::Conversation, messages::Message, streams::StreamCloser};
use napi::{
  bindgen_prelude::Result,
  threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use xmtp_mls::subscriptions::router_callbacks::stream_conversation_messages_with_callback_dispatch;

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream(
    &self,
    callback: ThreadsafeFunction<Message, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    let group = self.create_mls_group();
    let on_message =
      move |message: std::result::Result<_, xmtp_mls::subscriptions::SubscribeError>| {
        let status = callback.call(
          message
            .map(Message::from)
            .map_err(ErrorWrapper::from)
            .map_err(napi::Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
        tracing::info!("Stream status: {:?}", status);
      };
    let on_close = move || {
      on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
    };

    let handle = stream_conversation_messages_with_callback_dispatch(
      group.context.clone(),
      group.group_id,
      on_message,
      on_close,
    );
    Ok(StreamCloser::new(handle))
  }
}
