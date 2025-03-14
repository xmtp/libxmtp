use futures::FutureExt;
use std::future::Future;
use wasm_bindgen::prelude::*;
use xmtp_mls::storage::{OpfsSAHError, OpfsSAHPoolUtil};

#[wasm_bindgen]
pub struct Opfs;

#[wasm_bindgen]
impl Opfs {
  /// Check if the global OPFS object has been initialized
  #[wasm_bindgen]
  pub fn exists() -> bool {
    xmtp_mls::storage::SQLITE.get().is_some()
  }

  /// gets the error from Opfs, if any.
  #[wasm_bindgen]
  pub fn error() -> Option<String> {
    if let Some(Err(e)) = xmtp_mls::storage::SQLITE.get() {
      Some(e.to_string())
    } else {
      None
    }
  }

  #[wasm_bindgen(js_name = "wipeFiles")]
  pub async fn wipe_files() -> Result<(), JsError> {
    opfs_op_async(move |u| async move { u.wipe_files().await }).await
  }

  /// If a virtual file exists with the given name, disassociates it from the pool and returns true, else returns false without side effects.
  #[wasm_bindgen]
  pub fn rm(name: &str) -> Result<bool, JsError> {
    opfs_op(|u| u.unlink(name))
  }

  #[wasm_bindgen(js_name = "getFileNames")]
  pub fn ls() -> Vec<String> {
    opfs_op(|u| Ok(u.get_file_names())).expect("get_file_names is infallible")
  }

  #[wasm_bindgen(js_name = "importDb")]
  pub fn import_db(path: &str, bytes: &[u8]) -> Result<(), JsError> {
    opfs_op(|u| u.import_db(path, bytes))
  }

  #[wasm_bindgen(js_name = "exportFile")]
  pub fn export_file(name: &str) -> Result<Vec<u8>, JsError> {
    opfs_op(|u| u.export_file(name))
  }

  #[wasm_bindgen(js_name = "getFileCount")]
  pub fn get_file_count() -> u32 {
    opfs_op(|u| Ok(u.get_file_count())).expect("get_file_count is infallible")
  }
}

fn opfs_op<F, T>(f: F) -> Result<T, JsError>
where
  F: Fn(&OpfsSAHPoolUtil) -> Result<T, OpfsSAHError>,
{
  opfs_op_async(|opfs| async { f(opfs) })
    .now_or_never()
    .expect("sync op must resolve immediately")
}

async fn opfs_op_async<'a, F, Fut, T>(f: F) -> Result<T, JsError>
where
  F: Fn(&'a OpfsSAHPoolUtil) -> Fut,
  Fut: Future<Output = Result<T, OpfsSAHError>> + 'a,
{
  if let Some(pool) = xmtp_mls::storage::SQLITE.get() {
    match pool {
      Ok(p) => Ok(f(p).await?),
      Err(e) => Err(JsError::new(&e.to_string())),
    }
  } else {
    Err(JsError::new("no pool initialized"))
  }
}
