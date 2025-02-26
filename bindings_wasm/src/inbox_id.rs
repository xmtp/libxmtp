use crate::identity::{PublicIdentifier, RootIdentifier};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api::{strategies, ApiClientWrapper, ApiIdentifier};
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_id::associations::PublicIdentifier as XmtpPublicIdentifier;

#[wasm_bindgen(js_name = getInboxIdForAddress)]
pub async fn get_inbox_id_for_address(
  host: String,
  account_identifier: PublicIdentifier,
) -> Result<Option<String>, JsError> {
  let api_client = ApiClientWrapper::new(
    XmtpHttpApiClient::new(host.clone(), "0.0.0".into())?.into(),
    strategies::exponential_cooldown(),
  );

  let ident: XmtpPublicIdentifier = account_identifier.clone().try_into()?;
  let api_ident: ApiIdentifier = ident.into();
  let results = api_client
    .get_inbox_ids(vec![api_ident.clone()])
    .await
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

  Ok(results.get(&api_ident).cloned())
}

#[wasm_bindgen(js_name = generateInboxId)]
pub fn generate_inbox_id(account_identifier: RootIdentifier) -> Result<String, JsError> {
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  let ident: XmtpPublicIdentifier = account_identifier.to_public().try_into()?;
  Ok(
    ident
      .inbox_id(1)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?,
  )
}
