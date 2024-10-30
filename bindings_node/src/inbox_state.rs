use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::{AssociationState, MemberIdentifier};

use crate::{mls_client::NapiClient, ErrorWrapper};

#[napi(object)]
pub struct NapiInstallation {
  pub id: String,
  pub client_timestamp_ns: Option<BigInt>,
}

#[napi(object)]
pub struct NapiInboxState {
  pub inbox_id: String,
  pub recovery_address: String,
  pub installations: Vec<NapiInstallation>,
  pub account_addresses: Vec<String>,
}

impl From<AssociationState> for NapiInboxState {
  fn from(state: AssociationState) -> Self {
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_address: state.recovery_address().to_string(),
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Address(_) => None,
          MemberIdentifier::Installation(inst) => Some(NapiInstallation {
            id: ed25519_public_key_to_address(inst.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns.map(BigInt::from),
          }),
        })
        .collect(),
      account_addresses: state.account_addresses(),
    }
  }
}

#[napi]
impl NapiClient {
  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[napi]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<NapiInboxState> {
    let state = self
      .inner_client()
      .inbox_state(refresh_from_network)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<NapiInboxState> {
    let conn = self
      .inner_client()
      .store()
      .conn()
      .map_err(ErrorWrapper::from)?;
    let state = self
      .inner_client()
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }
}
