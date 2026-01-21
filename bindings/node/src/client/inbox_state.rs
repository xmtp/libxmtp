use crate::ErrorWrapper;
use crate::client::Client;
use crate::inbox_state::{InboxState, KeyPackageStatus};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::collections::HashMap;
use xmtp_id::InboxId;

#[napi]
impl Client {
  #[napi]
  pub async fn addresses_from_inbox_id(
    &self,
    refresh_from_network: bool,
    inbox_ids: Vec<String>,
  ) -> Result<Vec<InboxState>> {
    let state = self
      .inner_client
      .inbox_addresses(
        refresh_from_network,
        inbox_ids.iter().map(String::as_str).collect(),
      )
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into_iter().map(Into::into).collect())
  }

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
    let conn = self.inner_client().context.store().db();
    let state = self
      .inner_client()
      .identity_updates()
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn fetch_inbox_updates_count(
    &self,
    refresh_from_network: bool,
    inbox_ids: Vec<String>,
  ) -> Result<HashMap<InboxId, u32>> {
    let ids = inbox_ids.iter().map(AsRef::as_ref).collect();
    let res = self
      .inner_client()
      .fetch_inbox_updates_count(refresh_from_network, ids)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(res)
  }

  #[napi]
  pub async fn fetch_own_inbox_updates_count(&self, refresh_from_network: bool) -> Result<u32> {
    let res = self
      .inner_client()
      .fetch_own_inbox_updates_count(refresh_from_network)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(res)
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
