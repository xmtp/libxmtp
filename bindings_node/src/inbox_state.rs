use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::{AssociationState, MemberIdentifier};

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
