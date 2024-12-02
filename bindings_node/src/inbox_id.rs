use crate::ErrorWrapper;
use napi::bindgen_prelude::Result;
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use std::sync::Arc;
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_id::associations::MemberIdentifier;
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
    TonicApiClient::create(host, is_secure)
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
pub fn generate_inbox_id(account_address: String) -> Result<String> {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  let result = xmtp_id_generate_inbox_id(&account_address, &1).map_err(ErrorWrapper::from)?;
  Ok(result)
}

#[napi]
pub async fn is_installation_authorized(
  host: String,
  inbox_id: String,
  installation_id: Uint8Array,
) -> Result<bool> {
  is_member_of_association_state(
    &host,
    &inbox_id,
    &MemberIdentifier::Installation(installation_id.to_vec()),
  )
  .await
}

#[napi]
pub async fn is_address_authorized(
  host: String,
  inbox_id: String,
  address: String,
) -> Result<bool> {
  is_member_of_association_state(
    &host,
    &inbox_id,
    &MemberIdentifier::Address(address.to_lowercase()),
  )
  .await
}

async fn is_member_of_association_state(
  host: &str,
  inbox_id: &str,
  identifier: &MemberIdentifier,
) -> Result<bool> {
  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;
  let api_client = ApiClientWrapper::new(Arc::new(api_client), Retry::default());

  let is_member = xmtp_mls::identity_updates::is_member_of_association_state(
    &api_client,
    inbox_id,
    identifier,
    None,
  )
  .await
  .map_err(ErrorWrapper::from)?;

  Ok(is_member)
}
