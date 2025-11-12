use crate::identity::Identifier;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::{MessageBackendBuilder, TrackedStatsClient};
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_proto::types::ApiIdentifier;

#[wasm_bindgen(js_name = getInboxIdForIdentifier)]
pub async fn get_inbox_id_for_identifier(
  #[wasm_bindgen(js_name = host)] v3_host: String,
  #[wasm_bindgen(js_name = gatewayHost)] gateway_host: Option<String>,
  #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: Identifier,
) -> Result<Option<String>, JsError> {
  let backend = MessageBackendBuilder::default()
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .is_secure(true)
    .build()
    .map_err(|e| JsError::new(&e.to_string()))?;
  let api_client = ApiClientWrapper::new(
    TrackedStatsClient::new(backend),
    strategies::exponential_cooldown(),
  );

  let ident: XmtpIdentifier = account_identifier.clone().try_into()?;
  let api_ident: ApiIdentifier = ident.into();
  let results = api_client
    .get_inbox_ids(vec![api_ident.clone()])
    .await
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

  Ok(results.get(&api_ident).cloned())
}

#[wasm_bindgen(js_name = generateInboxId)]
pub fn generate_inbox_id(
  #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: Identifier,
) -> Result<String, JsError> {
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  let ident: XmtpIdentifier = account_identifier.try_into()?;

  ident
    .inbox_id(1)
    .map_err(|e| JsError::new(format!("{}", e).as_str()))
}
