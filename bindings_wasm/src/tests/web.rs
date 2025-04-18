use super::*;
use crate::opfs::Opfs;

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
pub async fn test_create_client() {
  create_test_client(None).await;
}

#[wasm_bindgen_test]
pub async fn wipe_client_files() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  let count = Opfs::get_file_count().unwrap();
  assert_eq!(count, 1);

  let files = Opfs::ls().unwrap();
  tracing::info!("Files");
  for file in files.iter() {
    tracing::info!("file: {}", file);
    Opfs::rm(file).unwrap();
  }
  assert_eq!(Opfs::get_file_count().unwrap(), 0);
}

#[wasm_bindgen_test]
async fn it_should_exist() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  assert!(Opfs::exists());
}

#[wasm_bindgen_test]
async fn it_should_not_have_error() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  assert_eq!(Opfs::error(), None);
}

#[wasm_bindgen_test]
async fn it_should_have_files() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  let files = Opfs::ls().unwrap();
  let file_count = Opfs::get_file_count().unwrap();
  assert_eq!(file_count, 1);
}

#[wasm_bindgen_test]
async fn it_should_have_capacity() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  assert_eq!(Opfs::get_capacity().unwrap(), 6);
}

#[wasm_bindgen_test]
async fn it_should_manage_capacity() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  Opfs::add_capacity(1).await.unwrap();
  assert_eq!(Opfs::get_capacity().unwrap(), 7);
  Opfs::reduce_capacity(2).await.unwrap();
  assert_eq!(Opfs::get_capacity().unwrap(), 5);
  Opfs::add_capacity(1).await.unwrap();
  assert_eq!(Opfs::get_capacity().unwrap(), 6);
}

#[wasm_bindgen_test]
async fn it_should_get_file_names() {
  Opfs::init_opfs_clean_slate().await;
  let _ = create_test_client(Some("test-sah-db".to_string())).await;
  let files: Vec<String> = Opfs::ls().unwrap();
  assert_eq!(files, vec!["/test-sah-db".to_string()]);
  assert_eq!(Opfs::get_file_count().unwrap(), 1);
}
