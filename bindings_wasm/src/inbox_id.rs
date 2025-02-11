use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api::ApiClientWrapper;
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_common::retry::Retry;
use xmtp_common::ExponentialBackoff;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;

fn retry_strategy() -> Retry<ExponentialBackoff, ExponentialBackoff> {
  let cooldown = ExponentialBackoff::builder()
    .duration(std::time::Duration::from_secs(3))
    .multiplier(3)
    .max_jitter(std::time::Duration::from_millis(100))
    .total_wait_max(std::time::Duration::from_secs(120))
    .build();

  xmtp_common::Retry::builder()
    .with_cooldown(cooldown)
    .build()
}

#[wasm_bindgen(js_name = getInboxIdForAddress)]
pub async fn get_inbox_id_for_address(
  host: String,
  account_address: String,
) -> Result<Option<String>, JsError> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(
    XmtpHttpApiClient::new(host.clone(), "0.0.0".into())?.into(),
    retry_strategy(),
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
