use crate::ErrorWrapper;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::retry::Retry;

#[napi]
pub async fn get_inbox_id_for_address(
  host: String,
  is_secure: bool,
  account_address: String,
) -> Result<Option<String>> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(
    TonicApiClient::create(host.clone(), is_secure)
      .await
      .map_err(ErrorWrapper::from)?
      .into(),
    Retry::default(),
  );

  let results = api_client
    .get_inbox_ids(vec![account_address.clone()])
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(results.get(&account_address).cloned())
}

#[napi]
pub fn generate_inbox_id(account_address: String) -> String {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  xmtp_id_generate_inbox_id(&account_address, &1)
}
