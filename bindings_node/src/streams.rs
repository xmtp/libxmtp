use std::sync::Arc;
use tokio::{sync::Mutex, task::AbortHandle};
use xmtp_mls::{client::ClientError, subscriptions::StreamHandle};
use napi::bindgen_prelude::Error;

use napi_derive::napi;

#[napi]
pub struct NapiStreamCloser {
    #[allow(clippy::type_complexity)]
    handle: Arc<Mutex<Option<StreamHandle<Result<(), ClientError>>>>>,
    // for convenience, does not require locking mutex.
    abort_handle: Arc<AbortHandle>,
}

impl NapiStreamCloser {
    pub fn new(handle: StreamHandle<Result<(), ClientError>>) -> Self {
        Self {
            abort_handle: Arc::new(handle.handle.abort_handle()),
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }
}

impl From<StreamHandle<Result<(), ClientError>>> for NapiStreamCloser {
    fn from(handle: StreamHandle<Result<(), ClientError>>) -> Self {
        NapiStreamCloser::new(handle) 
    }
}

#[napi]
impl NapiStreamCloser {
    /// Signal the stream to end
    /// Does not wait for the stream to end.
    #[napi]
    pub fn end(&self) {
        self.abort_handle.abort();
    }

    /// End the stream and `await` for it to shutdown
    /// Returns the `Result` of the task.
    #[napi]
    pub async fn end_and_wait(&self) -> Result<(), Error> {
        if self.abort_handle.is_finished() {
            return Ok(());
        }

        let mut handle = self.handle.lock().await;
        let handle = handle.take();
        if let Some(h) = handle {
            h.handle.abort();
            let join_result = h.handle.await;
            if matches!(join_result, Err(ref e) if !e.is_cancelled()) {
                return Err(Error::from_reason(
                    format!("subscription event loop join error {}", join_result.unwrap_err())
                ));
            }
        } else {
            log::warn!("subscription already closed");
        }
        Ok(())
    }
    
    /// Checks if this stream is closed
    #[napi]
    pub fn is_closed(&self) -> bool {
        self.abort_handle.is_finished()
    }
}
