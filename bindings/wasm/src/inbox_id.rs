use crate::ErrorWrapper;
use crate::client::backend::Backend;
use crate::identity::Identifier;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_proto::types::ApiIdentifier;

#[wasm_bindgen(js_name = getInboxIdForIdentifier)]
pub async fn get_inbox_id_for_identifier(
  backend: &Backend,
  #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: Identifier,
) -> Result<Option<String>, JsError> {
  let api_client = MessageBackendBuilder::default()
    .from_bundle(backend.bundle.clone())
    .map_err(ErrorWrapper::js)?;
  let api = ApiClientWrapper::new(api_client, strategies::exponential_cooldown());

  let ident: XmtpIdentifier = account_identifier.clone().try_into()?;
  let api_ident: ApiIdentifier = ident.into();
  let results = api
    .get_inbox_ids(vec![api_ident.clone()])
    .await
    .map_err(ErrorWrapper::js)?;

  Ok(results.get(&api_ident).cloned())
}

#[wasm_bindgen(js_name = generateInboxId)]
pub fn generate_inbox_id(
  #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: Identifier,
  nonce: Option<u64>,
) -> Result<String, JsError> {
  let ident: XmtpIdentifier = account_identifier.try_into()?;

  ident.inbox_id(nonce.unwrap_or(1)).map_err(ErrorWrapper::js)
}
