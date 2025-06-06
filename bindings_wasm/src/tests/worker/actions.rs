use futures::{stream::LocalBoxStream, Stream};
use pin_project_lite::pin_project;
use speedy::{Readable, Writable};
use std::task::{ready, Poll};
use wasm_bindgen::prelude::*;
use xmtp_mls::subscriptions::SubscribeError;

use crate::{client::RustMlsGroup, conversation::Conversation, conversations::ConversationType};

#[wasm_bindgen]
#[derive(Readable, Writable)]
pub struct ClientInfo {
  installation_id: String,
  inbox_id: String,
}

impl ClientInfo {
  pub fn new(installation_id: String, inbox_id: String) -> Self {
    Self {
      installation_id,
      inbox_id,
    }
  }

  pub fn inbox_id(&self) -> &str {
    &self.inbox_id
  }

  pub fn installation_id(&self) -> &str {
    &self.installation_id
  }
}

#[wasm_bindgen]
#[derive(Clone, Readable, Writable)]
pub struct ConversationHandle {
  group_id: String,
  created_at_ns: i64,
}

#[wasm_bindgen]
impl ConversationHandle {
  pub fn new(group_id: String, created_at_ns: i64) -> Self {
    Self {
      group_id,
      created_at_ns,
    }
  }

  #[wasm_bindgen]
  pub fn group_id(&self) -> String {
    self.group_id.clone()
  }

  #[wasm_bindgen]
  pub fn created_at_ns(&self) -> i64 {
    self.created_at_ns
  }
}

#[derive(Readable, Writable)]
pub enum Input {
  ClientInfo,
  CreateGroupByInboxIds { inbox_ids: Vec<String> },
  StreamConversations { kind: Option<ConversationType> },
  Die,
}

#[wasm_bindgen]
#[derive(Readable, Writable)]
pub enum Output {
  StreamConversations,
}

// JS-Compatible stream
pin_project! {
    pub struct ConversationStream {
      #[pin] stream: LocalBoxStream<'static, Result<RustMlsGroup, SubscribeError>>,
    }
}

// the type signature must match Result<JsValue, JsValue> so that we can use
// ReadableStream from the 'wasm-streams' crate
// https://docs.rs/wasm-streams/latest/wasm_streams/readable/struct.ReadableStream.html#method.from_stream
impl Stream for ConversationStream {
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
