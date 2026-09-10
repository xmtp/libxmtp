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
/// Old objects for the deleted file cannot reconnect.
#[wasm_bindgen(js_name = opfsDeleteFile)]
pub async fn delete_file(filename: String) -> Result<bool, JsError> {
  xmtp_db::database::delete_opfs_database(&filename)
    .await
    .map_err(crate::ErrorWrapper::js)
}

/// Delete all database files from OPFS.
/// Note: All databases must be closed before calling this function.
/// Old persistent database objects cannot reconnect after the clear starts.
#[wasm_bindgen(js_name = opfsClearAll)]
pub async fn clear_all() -> Result<(), JsError> {
  xmtp_db::database::clear_opfs_databases()
    .await
    .map_err(crate::ErrorWrapper::js)
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
/// The destination must not exist. Close and delete an old target separately.
/// The input must contain a current libxmtp SQLite database.
/// Import rotates the database identity before it exposes the restored copy.
/// Old cursors, delivery tokens, and target database objects cannot be reused.
#[wasm_bindgen(js_name = opfsImportDb)]
pub async fn import_db(filename: String, data: Uint8Array) -> Result<(), JsError> {
  xmtp_db::database::import_opfs_database(&filename, data.to_vec().as_slice())
    .await
    .map_err(crate::ErrorWrapper::js)
}
