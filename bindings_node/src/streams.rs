use napi::bindgen_prelude::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use xmtp_mls::{
  client::ClientError, AbortHandle, GenericStreamHandle, StreamHandle as XmtpStreamHandle,
  StreamHandleError,
};

use napi_derive::napi;

type StreamHandle = Box<GenericStreamHandle<Result<(), ClientError>>>;

#[napi]
pub struct StreamCloser {
  handle: Arc<Mutex<Option<StreamHandle>>>,
  abort: Arc<Box<dyn AbortHandle>>,
}

impl StreamCloser {
  pub fn new(
    handle: impl XmtpStreamHandle<StreamOutput = Result<(), ClientError>> + Send + Sync + 'static,
  ) -> Self {
    let abort = handle.abort_handle();
    Self {
      handle: Arc::new(Mutex::new(Some(Box::new(handle)))),
      abort: Arc::new(abort),
    }
  }
}

#[napi]
impl StreamCloser {
  /// Signal the stream to end
  /// Does not wait for the stream to end.
  #[napi]
  pub fn end(&self) {
    self.abort.end();
  }

  /// End the stream and `await` for it to shutdown
  /// Returns the `Result` of the task.
  /// End the stream and asyncronously wait for it to shutdown
  #[napi]
  pub async fn end_and_wait(&self) -> Result<(), Error> {
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

  /// Checks if this stream is closed
  #[napi]
  pub fn is_closed(&self) -> bool {
    self.abort.is_finished()
  }
}
