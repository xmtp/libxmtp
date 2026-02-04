use crate::{ErrorWrapper, identity::Identifier};
use napi::bindgen_prelude::{BigInt, Result, Uint8Array};
use napi_derive::napi;
use std::sync::Arc;
use xmtp_api::ApiClientWrapper;
use xmtp_api::strategies;
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_api_d14n::TrackedStatsClient;
use xmtp_db::EncryptedMessageStore;
use xmtp_db::NativeDb;
use xmtp_id::associations::{AssociationState, MemberIdentifier, ident};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::client::inbox_addresses_with_verifier;
use xmtp_mls::verified_key_package_v2::{VerifiedKeyPackageV2, VerifiedLifetime};

#[napi(object)]
pub struct Installation {
  pub bytes: Uint8Array,
  pub client_timestamp_ns: Option<BigInt>,
  pub id: String,
}

#[napi(object)]
pub struct InboxState {
  pub inbox_id: String,
  pub recovery_identifier: Identifier,
  pub installations: Vec<Installation>,
  pub identifiers: Vec<Identifier>,
}

#[napi(object)]
pub struct KeyPackageStatus {
  pub lifetime: Option<Lifetime>,
  pub validation_error: Option<String>,
}

#[napi(object)]
pub struct Lifetime {
  pub not_before: BigInt,
  pub not_after: BigInt,
}

impl From<AssociationState> for InboxState {
  fn from(state: AssociationState) -> Self {
    let ident: Identifier = state.recovery_identifier().clone().into();
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_identifier: ident,
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Ethereum(_) => None,
          MemberIdentifier::Passkey(_) => None,
          MemberIdentifier::Installation(ident::Installation(key)) => Some(Installation {
            bytes: Uint8Array::from(key.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns.map(BigInt::from),
            id: hex::encode(key),
          }),
        })
        .collect(),
      identifiers: state.identifiers().into_iter().map(Into::into).collect(),
    }
  }
}

impl From<VerifiedLifetime> for Lifetime {
  fn from(lifetime: VerifiedLifetime) -> Self {
    Self {
      not_before: BigInt::from(lifetime.not_before),
      not_after: BigInt::from(lifetime.not_after),
    }
  }
}

impl From<VerifiedKeyPackageV2> for KeyPackageStatus {
  fn from(key_package: VerifiedKeyPackageV2) -> Self {
    Self {
      lifetime: key_package.life_time().map(Into::into),
      validation_error: None,
    }
  }
}

#[allow(dead_code)]
#[napi]
pub async fn fetch_inbox_states_by_inbox_ids(
  v3_host: String,
  gateway_host: Option<String>,
  inbox_ids: Vec<String>,
) -> Result<Vec<InboxState>> {
  let backend = MessageBackendBuilder::default()
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .is_secure(true)
    .build()
    .map_err(ErrorWrapper::from)?;
  let backend = TrackedStatsClient::new(backend);

  let api = ApiClientWrapper::new(Arc::new(backend), strategies::exponential_cooldown());
  let scw_verifier = Arc::new(Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>);

  let db = NativeDb::builder()
    .ephemeral()
    .build_unencrypted()
    .map_err(ErrorWrapper::from)?;
  let store = EncryptedMessageStore::new(db).map_err(ErrorWrapper::from)?;

  let state = inbox_addresses_with_verifier(
    &api.clone(),
    &store.db(),
    inbox_ids.iter().map(String::as_str).collect(),
    &scw_verifier,
  )
  .await
  .map_err(ErrorWrapper::from)?;
  Ok(state.into_iter().map(Into::into).collect())
}
