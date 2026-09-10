use napi::bindgen_prelude::Error;
use napi_derive::napi;
use std::sync::Arc;
use tokio::sync::Mutex;
use xmtp_common::{
  AbortHandle, GenericStreamHandle, StreamHandle as XmtpStreamHandle, StreamHandleError,
};
use xmtp_mls::subscriptions::SubscribeError;

type StreamHandle = Box<GenericStreamHandle<Result<(), SubscribeError>>>;

#[napi]
pub struct StreamCloser {
  handle: Arc<Mutex<Option<StreamHandle>>>,
  abort: Arc<Box<dyn AbortHandle>>,
  message_control: Option<xmtp_mls::subscriptions::message_reader::MessageReaderControl>,
}

impl StreamCloser {
  pub fn new(
    handle: impl XmtpStreamHandle<StreamOutput = Result<(), SubscribeError>> + 'static,
  ) -> Self {
    let abort = handle.abort_handle();
    Self {
      handle: Arc::new(Mutex::new(Some(Box::new(handle)))),
      abort: Arc::new(abort),
      message_control: None,
    }
  }

  pub fn new_message(
    handle: impl XmtpStreamHandle<StreamOutput = Result<(), SubscribeError>> + 'static,
    control: xmtp_mls::subscriptions::message_reader::MessageReaderControl,
  ) -> Self {
    let mut closer = Self::new(handle);
    closer.message_control = Some(control);
    closer
  }
}

impl Drop for StreamCloser {
  fn drop(&mut self) {
    if let Some(control) = &self.message_control {
      control.close();
      self.abort.end();
    }
  }
}

#[napi]
impl StreamCloser {
  /// Signal the stream to end
  /// Does not wait for the stream to end.
  #[napi]
  pub fn end(&self) {
    if let Some(control) = &self.message_control {
      control.close();
    }
    self.abort.end();
  }

  /// End the stream and `await` for it to shutdown
  /// Returns the `Result` of the task.
  /// End the stream and asynchronously wait for it to shutdown
  #[napi]
  #[xmtp_common::err_span]
  pub async fn end_and_wait(&self) -> Result<(), Error> {
    if let Some(control) = &self.message_control {
      control.close();
    }
    use StreamHandleError::*;
    if self.abort.is_finished() {
      return Ok(());
    }

    let mut stream_handle = self.handle.lock().await;
    let stream_handle = stream_handle.take();

    if let Some(mut h) = stream_handle {
      match h.end_and_wait().await {
        Err(Cancelled) => Ok(()),
        Err(Panicked(msg)) => Err(Error::from_reason(msg)),
        Ok(t) => t.map_err(|e| Error::from_reason(e.to_string())),
        Err(e) => Err(Error::from_reason(format!("error joining task {}", e))),
      }
    } else {
      tracing::warn!("subscription already closed");
      Ok(())
    }
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn wait_for_ready(&self) -> Result<(), Error> {
    let mut stream_handle = self.handle.lock().await;
    futures::future::OptionFuture::from((*stream_handle).as_mut().map(|s| s.wait_for_ready()))
      .await;
    Ok(())
  }

  /// Checks if this stream is closed
  #[napi]
  pub fn is_closed(&self) -> bool {
    self.abort.is_finished()
  }

  #[napi]
  pub fn catch_up_snapshot(&self) -> Option<crate::message_delivery::MessageCatchUp> {
    self
      .message_control
      .as_ref()
      .map(|control| control.catch_up_snapshot().into())
  }

  #[napi]
  pub async fn catch_up_changed(&self) -> Option<crate::message_delivery::MessageCatchUp> {
    let control = self.message_control.as_ref()?;
    control.changed().await;
    Some(control.catch_up_snapshot().into())
  }
}
