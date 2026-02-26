use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use xmtp_db::database::{get_sqlite, init_sqlite};

/// Initialize the OPFS SQLite VFS if not already initialized.
/// This must be called before using other OPFS functions.
#[wasm_bindgen(js_name = opfsInit)]
pub async fn init_opfs() -> Result<(), JsError> {
  init_sqlite().await;
  if let Some(Err(e)) = get_sqlite() {
    return Err(JsError::new(&format!("Failed to initialize OPFS: {e}")));
  }
  Ok(())
}

/// List all database files stored in OPFS.
/// Returns an array of file names.
#[wasm_bindgen(js_name = opfsListFiles)]
pub async fn list_files() -> Result<Vec<String>, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => Ok(util.list()),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Check if a database file exists in OPFS.
#[wasm_bindgen(js_name = opfsFileExists)]
pub async fn file_exists(filename: String) -> Result<bool, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => util
      .exists(&filename)
      .map_err(|e| JsError::new(&format!("Failed to check file existence: {e}"))),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Delete a specific database file from OPFS.
/// Returns true if the file was deleted, false if it didn't exist.
/// Note: The database must be closed before calling this function.
#[wasm_bindgen(js_name = opfsDeleteFile)]
pub async fn delete_file(filename: String) -> Result<bool, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => util
      .delete_db(&filename)
      .map_err(|e| JsError::new(&format!("Failed to delete file: {e}"))),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Delete all database files from OPFS.
/// Note: All databases must be closed before calling this function.
#[wasm_bindgen(js_name = opfsClearAll)]
pub async fn clear_all() -> Result<(), JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => util
      .clear_all()
      .await
      .map_err(|e| JsError::new(&format!("Failed to clear all files: {e}"))),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Get the number of database files stored in OPFS.
#[wasm_bindgen(js_name = opfsFileCount)]
pub async fn file_count() -> Result<u32, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => Ok(util.count()),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Get the current capacity of the OPFS file pool.
#[wasm_bindgen(js_name = opfsPoolCapacity)]
pub async fn pool_capacity() -> Result<u32, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => Ok(util.get_capacity()),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Export a database file from OPFS as a byte array.
/// This can be used to backup or transfer a database.
/// Note: The database should be closed before exporting for consistency.
#[wasm_bindgen(js_name = opfsExportDb)]
pub async fn export_db(filename: String) -> Result<Uint8Array, JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => util
      .export_db(&filename)
      .map(|data: Vec<u8>| Uint8Array::from(data.as_slice()))
      .map_err(|e| JsError::new(&format!("Failed to export database: {e}"))),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}

/// Import a database from a byte array into OPFS.
/// This will overwrite any existing database with the same name.
/// The byte array must contain a valid SQLite database.
/// Note: Any existing database with the same name must be closed before importing.
#[wasm_bindgen(js_name = opfsImportDb)]
pub async fn import_db(filename: String, data: Uint8Array) -> Result<(), JsError> {
  init_sqlite().await;
  match get_sqlite() {
    Some(Ok(util)) => util
      .import_db(&filename, data.to_vec().as_slice())
      .map_err(|e| JsError::new(&format!("Failed to import database: {e}"))),
    Some(Err(e)) => Err(JsError::new(&format!("OPFS not initialized: {e}"))),
    None => Err(JsError::new("OPFS not initialized")),
  }
}
