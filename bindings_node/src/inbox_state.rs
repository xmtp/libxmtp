use std::collections::HashMap;

use crate::{client::Client, identity::Identifier, ErrorWrapper};
use napi::bindgen_prelude::{BigInt, Result, Uint8Array};
use napi_derive::napi;
use xmtp_id::associations::{ident, AssociationState, MemberIdentifier};
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

#[napi]
impl Client {
  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[napi]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<InboxState> {
    let state = self
      .inner_client()
      .inbox_state(refresh_from_network)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<InboxState> {
    let conn = self.inner_client().store().db();
    let state = self
      .inner_client()
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  /**
   * Get key package statuses for a list of installation IDs.
   *
   * Returns a JavaScript Object mapping installation ID strings to KeyPackageStatus objects.
   */
  #[napi]
  pub async fn get_key_package_statuses_for_installation_ids(
    &self,
    installation_ids: Vec<String>,
  ) -> Result<HashMap<String, KeyPackageStatus>> {
    // Convert String to Vec<u8>
    let installation_ids = installation_ids
      .into_iter()
      .map(hex::decode)
      .collect::<std::result::Result<Vec<Vec<u8>>, _>>()
      .map_err(ErrorWrapper::from)?;

    let key_package_results = self
      .inner_client()
      .get_key_packages_for_installation_ids(installation_ids)
      .await
      .map_err(ErrorWrapper::from)?;

    // Create a JavaScript Object to return
    let mut result_map = HashMap::new();

    for (installation_id, key_package_result) in key_package_results {
      let status = match key_package_result {
        Ok(key_package) => KeyPackageStatus::from(key_package),
        Err(e) => KeyPackageStatus {
          lifetime: None,
          validation_error: Some(e.to_string()),
        },
      };

      // Convert installation_id to hex string for JavaScript
      let id_key = hex::encode(&installation_id);
      result_map.insert(id_key, status);
    }

    Ok(result_map)
  }
}
