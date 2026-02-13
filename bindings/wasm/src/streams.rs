
use crate::client::RustMlsGroup;
use crate::conversation::Conversation;
use crate::enriched_message::DecodedMessage;
use crate::messages::Message;
use crate::user_preferences::UserPreferenceUpdate;
use futures::Stream;
use futures::{StreamExt, stream::LocalBoxStream};
use pin_project::pin_project;
use std::task::Poll;
use std::task::ready;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::*;
use xmtp_common::{
  AbortHandle, GenericStreamHandle, StreamHandle as XmtpStreamHandle, StreamHandleError,
};
use xmtp_mls::subscriptions::SubscribeError as XmtpSubscribeError;

type StreamHandle = Box<GenericStreamHandle<Result<(), XmtpSubscribeError>>>;

#[wasm_bindgen]
extern "C" {
  #[derive(Clone)]
  pub type StreamCallback;

  /// Js Fn to call on an item
  #[wasm_bindgen(structural, method)]
  pub fn on_message(this: &StreamCallback, item: Message);

  #[wasm_bindgen(structural, method)]
  pub fn on_consent_update(this: &StreamCallback, item: JsValue);

  #[wasm_bindgen(structural, method)]
  pub fn on_user_preference_update(this: &StreamCallback, item: Vec<UserPreferenceUpdate>);

  #[wasm_bindgen(structural, method)]
  pub fn on_conversation(this: &StreamCallback, item: Conversation);

  #[wasm_bindgen(structural, method)]
  pub fn on_message_deleted(this: &StreamCallback, message: DecodedMessage);

  /// Js Fn to call on error
  #[wasm_bindgen(structural, method)]
  pub fn on_error(this: &StreamCallback, error: JsError);

  #[wasm_bindgen(structural, method)]
  pub fn on_close(this: &StreamCallback);
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

// JS-Compatible Conversation stream
#[pin_project]
pub struct ConversationStream<'a> {
  #[pin]
  stream: LocalBoxStream<'a, Result<RustMlsGroup, XmtpSubscribeError>>,
}

impl<'a> ConversationStream<'a> {
  pub fn new(stream: impl Stream<Item = Result<RustMlsGroup, XmtpSubscribeError>> + 'a) -> Self {
    Self {
      stream: stream.boxed_local(),
    }
  }
}

// the type signature must match Result<JsValue, JsValue> so that we can use
// ReadableStream from the 'wasm-streams' crate
// https://docs.rs/wasm-streams/latest/wasm_streams/readable/struct.ReadableStream.html#method.from_stream
impl<'a> Stream for ConversationStream<'a> {
  type Item = Result<JsValue, JsValue>;

  fn poll_next(
    self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    let this = self.project();
    if let Some(item) = ready!(this.stream.poll_next(cx)) {
      match item {
        Ok(group) => Poll::Ready(Some(Ok(JsValue::from(Conversation::from(group))))),
        Err(e) => Poll::Ready(Some(Err(JsValue::from(JsError::new(&e.to_string()))))),
      }
    } else {
      Poll::Ready(None)
    }
  }
}
