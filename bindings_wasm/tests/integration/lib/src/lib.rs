use std::{cell::RefCell, rc::Rc};

use bindings_wasm::{
  conversations::ConversationType,
  tests::worker::actions::{ClientInfo, ConversationHandle, Input, Output},
};
use js_sys::{Array, Uint8Array};
use speedy::{Context, Endianness, LittleEndian, Readable, Writable};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use wasm_bindgen::{JsCast, link_to, prelude::*};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
  Blob, BlobPropertyBag, MessageEvent, ReadableStream, Url, Worker, WorkerOptions, WorkerType,
  window,
};

pub const NATIVE_ENDIAN: Endianness = Endianness::NATIVE;

fn create_worker() -> Worker {
  let origin = window()
    .expect("window to be available")
    .location()
    .origin()
    .expect("origin to be available");
  let mut options = WorkerOptions::new();
  options.set_type(WorkerType::Module);
  Worker::new_with_options("/lib/worker.js", &options).expect("failed to spawn worker")
}

#[wasm_bindgen]
pub struct XmtpClient {
  worker: Worker,
  queue: Rc<RefCell<ReceiverStream<MessageEvent>>>,
}

#[wasm_bindgen]
impl XmtpClient {
  #[wasm_bindgen(constructor)]
  pub async fn new() -> Self {
    let (tx, rx) = mpsc::channel(32);
    let mut queue = ReceiverStream::new(rx);
    // let worker = worker_new("/worker/worker.js");
    let worker = create_worker();
    let worker_clone = worker.clone();
    let t = tx.clone();
    let onmessage = Closure::wrap(Box::new(move |msg: MessageEvent| {
      let worker_clone = worker_clone.clone();
      tracing::info!("Got Message in Main Thread");
      t.try_send(msg).unwrap();
    }) as Box<dyn Fn(MessageEvent)>);
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
    // wait for it to send us an empty message
    queue.next().await.unwrap();
    Self {
      queue: Rc::new(RefCell::new(queue)),
      worker,
    }
  }

  #[wasm_bindgen]
  pub async fn info(&self) -> String {
    let result: ClientInfo = self.send(Input::ClientInfo).await;
    result.inbox_id().to_string()
  }

  #[wasm_bindgen]
  pub async fn create_group_by_inbox_ids(&self, inbox_ids: Vec<String>) -> ConversationHandle {
    self.send(Input::CreateGroupByInboxIds { inbox_ids }).await
  }

  #[wasm_bindgen]
  pub async fn stream_conversations(
    &self,
    conversation_type: Option<ConversationType>,
  ) -> web_sys::ReadableStream {
    let data = Uint8Array::from(
      Input::StreamConversations {
        kind: conversation_type,
      }
      .write_to_vec()
      .unwrap()
      .as_slice(),
    );
    self.worker.post_message(&JsValue::from(data)).unwrap();
    let msg: MessageEvent = {
      let mut queue = self.queue.borrow_mut();
      queue.next().await.unwrap()
    };
    msg.data().into()
    // let s = wasm_streams::ReadableStream::from_raw(s);
  }
}

impl XmtpClient {
  async fn send<'a, T: Readable<'a, LittleEndian>>(&self, msg: Input) -> T {
    let data = Uint8Array::from(msg.write_to_vec().unwrap().as_slice());
    self.worker.post_message(&JsValue::from(data)).unwrap();
    let mut queue = self.queue.borrow_mut();
    let msg = queue.next().await.unwrap();
    let data = Uint8Array::from(msg.data());
    T::read_from_buffer_copying_data(&data.to_vec()).unwrap()
  }
}

#[wasm_bindgen(start)]
pub async fn start_app() {
  console_error_panic_hook::set_once();
  xmtp_common::logger("warn");
  tracing::info!("App Started");
}
