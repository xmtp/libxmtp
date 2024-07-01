use std::sync::Arc;
use tokio::{sync::Mutex, task::{JoinHandle, AbortHandle}};
use xmtp_mls::client::ClientError;
use napi::bindgen_prelude::Error;

use napi_derive::napi;

#[napi]
pub struct NapiStreamCloser {
    handle: Arc<Mutex<Option<JoinHandle<Result<(), ClientError>>>>>,
    // for convenience, does not require locking mutex.
    abort_handle: Arc<AbortHandle>,
}

impl NapiStreamCloser {
    pub fn new(handle: JoinHandle<Result<(), ClientError>>) -> Self {
        Self {
            abort_handle: Arc::new(handle.abort_handle()),
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }
}

impl From<JoinHandle<Result<(), ClientError>>> for NapiStreamCloser {
    fn from(handle: JoinHandle<Result<(), ClientError>>) -> Self {
        NapiStreamCloser::new(handle) 
    }
}

#[napi]
impl NapiStreamCloser {
    /// Signal the stream to end
    /// Does not wait for the stream to end.
    pub fn end(&self) {
        self.abort_handle.abort();
    }

    /// End the stream and `await` for it to shutdown
    /// Returns the `Result` of the task.
    pub async fn end_and_wait(&self) -> Result<(), Error> {
        if self.abort_handle.is_finished() {
            return Ok(());
        }

        let mut handle = self.handle.lock().await;
        let handle = handle.take();
        if let Some(h) = handle {
            h.abort();
            let join_result = h.await;
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
    pub fn is_closed(&self) -> bool {
        self.abort_handle.is_finished()
    }
}
