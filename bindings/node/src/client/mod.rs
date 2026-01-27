use crate::ErrorWrapper;
use crate::conversations::Conversations;
use crate::identity::{Identifier, IdentityExt};
use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;
use xmtp_mls::Client as MlsClient;
use xmtp_mls::groups::MlsGroup;

mod consent_state;
pub mod create_client;
mod gateway_auth;
mod identity;
mod inbox_state;
pub mod options;
mod signatures;
mod stats;

pub type RustXmtpClient = MlsClient<xmtp_mls::MlsContext>;
pub type RustMlsGroup = MlsGroup<xmtp_mls::MlsContext>;

#[napi]
#[derive(Clone)]
pub struct Client {
  inner_client: Arc<RustXmtpClient>,
  account_identifier: Identifier,
  app_version: Option<String>,
}

#[napi]
impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  #[napi]
  pub fn app_version(&self) -> String {
    self.app_version.clone().unwrap_or_default()
  }

  #[napi]
  pub fn libxmtp_version(&self) -> String {
    env!("CARGO_PKG_VERSION").to_string()
  }

  #[napi]
  /// The resulting vec will be the same length as the input and should be zipped for the results.
  pub async fn can_message(
    &self,
    account_identities: Vec<Identifier>,
  ) -> Result<HashMap<String, bool>> {
    let ident = account_identities.to_internal()?;
    let results = self
      .inner_client
      .can_message(&ident)
      .await
      .map_err(ErrorWrapper::from)?;

    let results = results
      .into_iter()
      .map(|(k, v)| (format!("{k}"), v))
      .collect();

    Ok(results)
  }

  #[napi]
  pub fn conversations(&self) -> Conversations {
    Conversations::new(self.inner_client.clone())
  }

  #[napi]
  pub fn release_db_connection(&self) -> Result<()> {
    self
      .inner_client
      .release_db_connection()
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn db_reconnect(&self) -> Result<()> {
    self
      .inner_client
      .reconnect_db()
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }
}
