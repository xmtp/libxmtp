use bindings_wasm::mls_client::{create_client, get_inbox_id_for_address};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_mls_gateway::constants::ApiUrls;
use xmtp_cryptography::utils::{rng, LocalWallet};
use xmtp_id::InboxOwner;

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_browser);

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
  let inbox_id = get_inbox_id_for_address(host.clone(), account_address.clone())
    .await
    .unwrap_or_else(|e| panic!("Error getting inbox ID"));
  let client = create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.clone(),
    None,
    None,
  )
  .await;

  assert!(client.is_ok());
}
