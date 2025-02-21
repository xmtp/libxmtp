use bindings_wasm::client::LogLevel;
use bindings_wasm::client::{LogOptions, create_client};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_http::constants::ApiUrls;
use xmtp_cryptography::utils::{LocalWallet, rng};
use xmtp_id::InboxOwner;
use xmtp_id::associations::generate_inbox_id;

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console)]
  fn log(s: &str);
}

#[wasm_bindgen_test]
pub async fn test_create_client() {
  let wallet = LocalWallet::new(&mut rng());
  let account_address = wallet.get_address();
  let host = ApiUrls::LOCAL_ADDRESS.to_string();
  let inbox_id = generate_inbox_id(&account_address, &1);
  let client = create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.clone(),
    None,
    None,
    None,
    Some(LogOptions {
      structured: false,
      performance: false,
      level: Some(LogLevel::Info),
    }),
  )
  .await;
  if let Err(ref e) = client {
    tracing::info!("{:?}", e);
  }
  assert!(client.is_ok());
}
