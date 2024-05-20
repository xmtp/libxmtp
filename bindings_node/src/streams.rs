use std::sync::{
  atomic::{AtomicBool, Ordering},
  Arc, Mutex,
};
use tokio::sync::oneshot::Sender;

use napi_derive::napi;

#[napi]
pub struct NapiStreamCloser {
  close_fn: Arc<Mutex<Option<Sender<()>>>>,
  is_closed_atomic: Arc<AtomicBool>,
}

#[napi]
impl NapiStreamCloser {
  pub fn new(close_fn: Arc<Mutex<Option<Sender<()>>>>, is_closed_atomic: Arc<AtomicBool>) -> Self {
    Self {
      close_fn,
      is_closed_atomic,
    }
  }

  #[napi]
  pub fn end(&self) {
    match self.close_fn.lock() {
      Ok(mut close_fn_option) => {
        let _ = close_fn_option.take().map(|close_fn| close_fn.send(()));
      }
      _ => {}
    }
  }

  #[napi]
  pub fn is_closed(&self) -> bool {
    self.is_closed_atomic.load(Ordering::Relaxed)
  }
}
