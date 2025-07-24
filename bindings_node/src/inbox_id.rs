use crate::identity::Identifier;
use crate::ErrorWrapper;
use napi::bindgen_prelude::Result;
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use std::sync::Arc;
use xmtp_api::ApiIdentifier;
use xmtp_api::{strategies, ApiClientWrapper};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_id::associations::MemberIdentifier;

#[napi]
pub async fn get_inbox_id_for_identifier(
  host: String,
  is_secure: bool,
  identifier: Identifier,
) -> Result<Option<String>> {
  let client = TonicApiClient::create(&host, is_secure, None::<String>)
    .await
    .map_err(ErrorWrapper::from)?;
  // api rate limit cooldown period
  let api_client = ApiClientWrapper::new(client, strategies::exponential_cooldown());

  let identifier: xmtp_id::associations::Identifier = identifier.try_into()?;
  let api_ident: ApiIdentifier = identifier.into();
  let results = api_client
    .get_inbox_ids(vec![api_ident.clone()])
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(results.get(&api_ident).cloned())
}

#[napi]
pub fn generate_inbox_id(account_ident: Identifier) -> Result<String> {
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  let ident: XmtpIdentifier = account_ident.try_into()?;
  Ok(ident.inbox_id(1).map_err(ErrorWrapper::from)?)
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
    &MemberIdentifier::installation(installation_id.to_vec()),
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
    &MemberIdentifier::eth(address).map_err(ErrorWrapper::from)?,
  )
  .await
}

async fn is_member_of_association_state(
  host: &str,
  inbox_id: &str,
  identifier: &MemberIdentifier,
) -> Result<bool> {
  let api_client = TonicApiClient::create(host, true, None::<String>)
    .await
    .map_err(ErrorWrapper::from)?;
  let api_client = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());

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
