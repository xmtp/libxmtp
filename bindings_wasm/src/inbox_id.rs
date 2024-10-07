use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::retry::Retry;

#[wasm_bindgen(js_name = getInboxIdForAddress)]
pub async fn get_inbox_id_for_address(
  host: String,
  account_address: String,
) -> Result<Option<String>, JsError> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(
    XmtpHttpApiClient::new(host.clone()).unwrap(),
    Retry::default(),
  );

  let results = api_client
    .get_inbox_ids(vec![account_address.clone()])
    .await
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

  Ok(results.get(&account_address).cloned())
}

#[wasm_bindgen(js_name = generateInboxId)]
pub fn generate_inbox_id(account_address: String) -> String {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  xmtp_id_generate_inbox_id(&account_address, &1)
}
