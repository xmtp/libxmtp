mod web;
pub mod worker;

use crate::client::LogLevel;
use crate::client::{create_client, Client, LogOptions};
use crate::inbox_id::generate_inbox_id;
use crate::signatures::SignatureRequestType;
use alloy::signers::SignerSync;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_http::constants::ApiUrls;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::InboxOwner;

#[wasm_bindgen(js_name = createTestClient)]
pub async fn create_test_client() -> Client {
  // crate::opfs::Opfs::wipe_files().await.unwrap();
  let wallet = generate_local_wallet();
  let account_address = wallet.get_identifier().unwrap_throw();
  let host = ApiUrls::LOCAL_ADDRESS.to_string();
  let inbox_id = generate_inbox_id(account_address.clone().into());
  let mut client = create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.into(),
    None,
    None,
    None,
    Some(crate::client::DeviceSyncWorkerMode::Disabled),
    Some(LogOptions {
      structured: false,
      performance: true,
      level: Some(LogLevel::Info),
    }),
  )
  .await
  .unwrap();
  let text = client.create_inbox_signature_text().unwrap().unwrap();
  let signature = wallet.sign_message_sync(text.as_bytes()).unwrap();
  client
    .add_ecdsa_signature(
      SignatureRequestType::CreateInbox,
      Uint8Array::from(&signature.as_bytes()[..]),
    )
    .await
    .unwrap();
  client.register_identity().await.unwrap();
  client
}
