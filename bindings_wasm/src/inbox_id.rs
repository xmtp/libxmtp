use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api::{strategies, ApiClientWrapper};
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;

#[wasm_bindgen(js_name = getInboxIdForAddress)]
pub async fn get_inbox_id_for_address(
  host: String,
  account_address: String,
) -> Result<Option<String>, JsError> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(
    XmtpHttpApiClient::new(host.clone(), "0.0.0".into())?.into(),
    strategies::exponential_cooldown(),
  );

  let results = api_client
    .get_inbox_ids(vec![account_address.clone()])
    .await
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

  Ok(results.get(&account_address).cloned())
}

#[wasm_bindgen(js_name = generateInboxId)]
pub fn generate_inbox_id(account_address: String) -> Result<String, JsError> {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  let result = xmtp_id_generate_inbox_id(&account_address, &1)
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
  Ok(result)
}
