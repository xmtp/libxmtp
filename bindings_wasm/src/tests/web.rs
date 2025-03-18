use super::*;
use crate::opfs::Opfs;

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
pub async fn test_create_client() {
  create_test_client().await;
}

#[wasm_bindgen_test]
pub async fn wipe_client_files() {
  xmtp_mls::storage::init_sqlite().await;
  Opfs::wipe_files().await.unwrap();
  let client = create_test_client().await;
  let count = Opfs::get_file_count();
  assert_eq!(count, 1);

  let files = Opfs::ls();
  tracing::info!("Files");
  for file in files.iter() {
    tracing::info!("file: {}", file);
    Opfs::rm(file).unwrap();
  }
  assert_eq!(Opfs::get_file_count(), 0);
}
