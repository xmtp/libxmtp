use futures::FutureExt;
use std::future::Future;
use wasm_bindgen::prelude::*;
use xmtp_db::{OpfsSAHError, OpfsSAHPoolUtil};

#[wasm_bindgen]
pub struct Opfs;

#[wasm_bindgen]
impl Opfs {
  /// Check if the global OPFS object has been initialized
  #[wasm_bindgen]
  pub fn exists() -> bool {
    xmtp_db::SQLITE.get().is_some()
  }

  /// gets the error from Opfs, if any.
  #[wasm_bindgen]
  pub fn error() -> Option<String> {
    if let Some(Err(e)) = xmtp_db::SQLITE.get() {
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

  /// list files in current pool
  #[wasm_bindgen(js_name = "getFileNames")]
  pub fn ls() -> Vec<String> {
    opfs_op(|u| Ok(u.get_file_names())).expect("get_file_names is infallible")
  }

  /// import a db file at 'path'
  #[wasm_bindgen(js_name = "importDb")]
  pub fn import_db(path: &str, bytes: &[u8]) -> Result<(), JsError> {
    opfs_op(|u| u.import_db(path, bytes))
  }

  /// export db file with 'name'
  #[wasm_bindgen(js_name = "exportFile")]
  pub fn export_file(name: &str) -> Result<Vec<u8>, JsError> {
    opfs_op(|u| u.export_file(name))
  }

  /// get number of files in pool
  #[wasm_bindgen(js_name = "getFileCount")]
  pub fn get_file_count() -> u32 {
    opfs_op(|u| Ok(u.get_file_count())).expect("get_file_count is infallible")
  }

  #[wasm_bindgen(js_name = "getCapacity")]
  pub fn get_capacity() -> u32 {
    opfs_op(|u| Ok(u.get_capacity())).expect("get_capacity is infallible")
  }

  /// Adds n entries to the current pool.
  #[wasm_bindgen(js_name = "addCapacity")]
  pub async fn add_capacity(n: u32) -> Result<u32, JsError> {
    opfs_op_async(|u| u.add_capacity(n)).await
  }

  /// Removes up to n entries from the pool, with the caveat that it can only remove currently-unused entries.
  #[wasm_bindgen(js_name = "reduceCapacity")]
  pub async fn reduce_capacity(n: u32) -> Result<u32, JsError> {
    opfs_op_async(|u| u.reduce_capacity(n)).await
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
  if let Some(pool) = xmtp_db::SQLITE.get() {
    match pool {
      Ok(p) => Ok(f(p).await?),
      Err(e) => Err(JsError::new(&e.to_string())),
    }
  } else {
    Err(JsError::new("no pool initialized"))
  }
}
