use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsError;
use xmtp_mls::{
  subscriptions::SubscribeError as XmtpSubscribeError, AbortHandle, GenericStreamHandle,
  StreamHandle as XmtpStreamHandle, StreamHandleError,
};

type StreamHandle = Box<GenericStreamHandle<Result<(), XmtpSubscribeError>>>;

#[wasm_bindgen]
extern "C" {
  pub type StreamCallback;

  /// Js Fn to call on an item
  #[wasm_bindgen(structural, method)]
  pub fn on_item(this: &StreamCallback, item: JsValue);

  /// Js Fn to call on error
  #[wasm_bindgen(structural, method)]
  pub fn on_error(this: &StreamCallback, error: JsError);
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct StreamCloser {
  handle: Rc<RefCell<Option<StreamHandle>>>,
  abort: Rc<Box<dyn AbortHandle>>,
}

impl StreamCloser {
  pub fn new(
    handle: impl XmtpStreamHandle<StreamOutput = Result<(), XmtpSubscribeError>> + 'static,
  ) -> Self {
    let abort = handle.abort_handle();
    Self {
      handle: Rc::new(RefCell::new(Some(Box::new(handle)))),
      abort: Rc::new(abort),
    }
  }
}

#[wasm_bindgen]
impl StreamCloser {
  /// Signal the stream to end
  /// Does not wait for the stream to end.
  #[wasm_bindgen(js_name = "end")]
  pub fn end(&self) {
    self.abort.end();
  }

  /// End the stream and `await` for it to shutdown
  /// Returns the `Result` of the task.
  /// End the stream and asynchronously wait for it to shutdown
  #[wasm_bindgen(js_name = "endAndWait")]
  pub async fn end_and_wait(&self) -> Result<(), JsError> {
    use StreamHandleError::*;
    if self.abort.is_finished() {
      return Ok(());
    }

    let stream_handle = {
      let mut handle = self.handle.borrow_mut();
      handle.take()
    };

    if let Some(mut h) = stream_handle {
      match h.end_and_wait().await {
        Err(Cancelled) => Ok(()),
        Err(Panicked(msg)) => Err(JsError::new(&msg)),
        Ok(t) => t.map_err(|e| JsError::new(&e.to_string())),
        Err(e) => Err(JsError::new(&format!("error joining task {}", e))),
      }
    } else {
      tracing::warn!("subscription already closed");
      Ok(())
    }
  }

  #[wasm_bindgen(js_name = "waitForReady")]
  pub async fn wait_for_ready(&mut self) -> Result<(), JsError> {
    let mut opt = Rc::get_mut(&mut self.handle);
    let opt = opt
      .as_mut()
      .map(|h| h.get_mut().as_mut().map(|s| s.wait_for_ready()));
    futures::future::OptionFuture::from(opt.flatten()).await;
    Ok(())
  }

  /// Checks if this stream is closed
  #[wasm_bindgen(js_name = "isClosed")]
  pub fn is_closed(&self) -> bool {
    self.abort.is_finished()
  }
}
