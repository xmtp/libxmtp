mod web;

use crate::client::LogLevel;
use crate::client::{create_client, Client, LogOptions};
use crate::inbox_id::generate_inbox_id;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_http::constants::ApiUrls;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::InboxOwner;

async fn create_test_client() -> Client {
  // crate::opfs::Opfs::wipe_files().await.unwrap();
  let wallet = generate_local_wallet();
  let account_address = wallet.get_identifier().unwrap_throw();
  let host = ApiUrls::LOCAL_ADDRESS.to_string();
  let inbox_id = generate_inbox_id(account_address.clone().into());
  let db = xmtp_common::tmp_path();
  create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.into(),
    Some(db),
    None,
    None,
    None,
    Some(LogOptions {
      structured: false,
      performance: true,
      level: Some(LogLevel::Debug),
    }),
  )
  .await
  .unwrap()
}
