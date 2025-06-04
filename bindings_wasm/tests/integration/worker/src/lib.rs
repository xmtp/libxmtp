use bindings_wasm::client::LogLevel;
use bindings_wasm::tests::worker::actions::{Input, *};
use bindings_wasm::{client::Client, tests::create_test_client};
use js_sys::{Array, Uint8Array};
use speedy::{Readable, Writable};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

#[wasm_bindgen(start)]
pub async fn start_worker() {
  console_error_panic_hook::set_once();
  tracing::info!("WORKER STARTED");
  web_sys::console::log_1(&"worker starting".into());
  xmtp_client_worker().await
}

pub async fn xmtp_client_worker() {
  let scope = DedicatedWorkerGlobalScope::from(JsValue::from(js_sys::global()));
  let scope_clone = scope.clone();
  let this = create_test_client(LogLevel::Info).await;
  let onmessage = Closure::wrap(Box::new(move |msg: MessageEvent| {
    let client = this.clone();
    tracing::info!("[worker] got message");
    let data = Uint8Array::from(msg.data()).to_vec();
    let data: Input = Input::read_from_buffer(&data).unwrap();
    let s = scope_clone.clone();
    spawn_local(async move {
      let s = &s;
      event_loop(data, &s, &client).await;
    });
  }) as Box<dyn Fn(MessageEvent)>);
  scope.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
  onmessage.forget();

  scope
    .post_message(&Array::new().into())
    .expect("posting ready message succeeds");
}

async fn event_loop(input: Input, scope: &DedicatedWorkerGlobalScope, client: &Client) {
  match input {
    Input::ClientInfo => {
      let msg = ClientInfo::new(client.installation_id(), client.inbox_id());
      let msg = msg.write_to_vec().unwrap();
      let msg = Uint8Array::from(msg.as_slice());
      scope.post_message(&JsValue::from(msg)).unwrap()
    }
    Input::CreateGroupByInboxIds { inbox_ids } => {
      let group = client
        .conversations()
        .create_group_by_inbox_ids(inbox_ids, None)
        .await
        .unwrap();
      let group = ConversationHandle::new(group.id(), group.created_at_ns())
        .write_to_vec()
        .unwrap();
      let group = Uint8Array::from(group.as_slice());
      scope.post_message(&JsValue::from(group)).unwrap()
    }
    Input::StreamConversations { kind } => {
      let readable_stream = client
        .conversations()
        .stream_conversations_local(kind)
        .await
        .unwrap();
      web_sys::console::log_2(&"TRANSFERRING".into(), &readable_stream);
      scope
        .post_message_with_transfer(&readable_stream, &Array::of1(&readable_stream))
        .unwrap_throw();
    }
    Input::Die => todo!(),
  }
}
