use crate::{ErrorWrapper, conversation::Conversation, messages::Message, streams::StreamCloser};
use napi::{
  bindgen_prelude::Result,
  threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use xmtp_mls::groups::MlsGroup;

#[napi]
impl Conversation {
  #[napi]
  pub async fn stream(
    &self,
    callback: ThreadsafeFunction<Message, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    let group = self.create_mls_group();
    let stream_closer = MlsGroup::stream_with_callback(
      group.context.clone(),
      group.group_id.clone(),
      move |message| {
        let status = callback.call(
          message
            .map(Message::from)
            .map_err(ErrorWrapper::from)
            .map_err(napi::Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
        tracing::info!("Stream status: {:?}", status);
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }
}
