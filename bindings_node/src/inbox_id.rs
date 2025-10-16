use crate::ErrorWrapper;
use crate::identity::Identifier;
use napi::bindgen_prelude::Result;
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use std::sync::Arc;
use xmtp_api::ApiIdentifier;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_api_d14n::TrackedStatsClient;
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_id::associations::MemberIdentifier;

#[napi]
pub async fn get_inbox_id_for_identifier(
  v3_host: String,
  gateway_host: Option<String>,
  is_secure: bool,
  identifier: Identifier,
) -> Result<Option<String>> {
  let backend = MessageBackendBuilder::default()
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .is_secure(is_secure)
    .build()
    .map_err(ErrorWrapper::from)?;
  let backend = TrackedStatsClient::new(backend);

  // api rate limit cooldown period
  let api_client = ApiClientWrapper::new(backend, strategies::exponential_cooldown());

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
  gateway_host: Option<String>,
  inbox_id: String,
  installation_id: Uint8Array,
) -> Result<bool> {
  is_member_of_association_state(
    &host,
    gateway_host,
    &inbox_id,
    &MemberIdentifier::installation(installation_id.to_vec()),
  )
  .await
}

#[napi]
pub async fn is_address_authorized(
  host: String,
  gateway_host: Option<String>,
  inbox_id: String,
  address: String,
) -> Result<bool> {
  is_member_of_association_state(
    &host,
    gateway_host,
    &inbox_id,
    &MemberIdentifier::eth(address).map_err(ErrorWrapper::from)?,
  )
  .await
}

async fn is_member_of_association_state(
  v3_host: &str,
  gateway_host: Option<String>,
  inbox_id: &str,
  identifier: &MemberIdentifier,
) -> Result<bool> {
  let backend = MessageBackendBuilder::default()
    .maybe_gateway_host(gateway_host)
    .v3_host(v3_host)
    .is_secure(true)
    .build()
    .map_err(ErrorWrapper::from)?;
  let backend = TrackedStatsClient::new(backend);

  let api_client = ApiClientWrapper::new(Arc::new(backend), strategies::exponential_cooldown());

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
