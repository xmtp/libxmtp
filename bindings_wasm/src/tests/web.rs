use super::*;
use crate::opfs::Opfs;

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
pub async fn wipe_client_files() {
  xmtp_db::init_sqlite().await;
  Opfs::wipe_files().await.unwrap();
  let path = xmtp_common::tmp_path();
  let _client = create_test_client(Some(path)).await;
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

#[wasm_bindgen_test]
pub async fn can_stream_conversations() {
  let alix = create_test_client().await;
  let bo = create_test_client().await;
  let _ = alix
    .conversations()
    .create_group_by_inbox_ids(vec![bo.inbox_id()], None)
    .await
    .unwrap();
}
