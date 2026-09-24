use crate::ErrorWrapper;
use crate::client::Client;
use crate::identity::Identifier;
use crate::signatures::SignatureRequestHandle;
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;

use super::registration_visible::VisibilityConfirmationOptions;

#[napi]
impl Client {
  #[napi(getter)]
  pub fn account_identifier(&self) -> Identifier {
    self.account_identifier.clone()
  }

  #[napi]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id().to_string()
  }

  #[napi]
  pub fn is_registered(&self) -> Result<bool> {
    Ok(
      self
        .inner_client
        .is_registration_visible()
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn installation_id(&self) -> String {
    hex::encode(self.inner_client.installation_public_key())
  }

  #[napi]
  pub fn installation_id_bytes(&self) -> Uint8Array {
    self.inner_client.installation_public_key().into()
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn register_identity(
    &self,
    signature_request: &SignatureRequestHandle,
    visibility_confirmation_options: Option<VisibilityConfirmationOptions>,
  ) -> Result<()> {
    let _ = visibility_confirmation_options;
    if self.inner_client.identity().is_ready() {
      self
        .inner_client
        .ensure_registration_visible()
        .await
        .map_err(ErrorWrapper::from)?;
      return Ok(());
    }

    {
      let inner = signature_request.inner().lock().await;
      self
        .inner_client
        .register_identity(inner.clone())
        .await
        .map_err(ErrorWrapper::from)?;
    }

    Ok(())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn get_inbox_id_by_identity(&self, identifier: Identifier) -> Result<Option<String>> {
    let conn = self.inner_client().context.store().db();

    let inbox_id = self
      .inner_client
      .find_inbox_id_from_identifier(&conn, identifier.try_into()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(inbox_id)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Arc;
  use xmtp_db::{Fetch, identity::StoredIdentity, prelude::QueryIdentityUpdates};
  use xmtp_id::InboxOwner;
  use xmtp_mls::context::XmtpSharedContext;
  use xmtp_mls::{
    Client as MlsClient, identity::IdentityStrategy, utils::test::set_registration_cursor_for_test,
  };

  // verifies: IDENT-072
  #[xmtp_common::test(unwrap_try = true)]
  async fn register_after_unconfirmed_registration_waits() {
    xmtp_mls::tester!(alix, disable_workers);
    let signature = alix
      .identity_updates()
      .associate_identity(xmtp_cryptography::utils::generate_local_wallet().get_identifier()?)
      .await?;
    let handle = SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature)),
      alix.scw_verifier(),
    );
    let receipt = alix
      .context
      .db()
      .get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()];
    set_registration_cursor_for_test(&alix.context.db(), i64::MAX);
    let reopened = MlsClient::builder(IdentityStrategy::CachedOnly)
      .store(alix.context.store().clone())
      .api_client(alix.context.api().api_client.clone())
      .with_scw_verifier(alix.scw_verifier())
      .default_mls_store()?
      .with_disable_workers(true)
      .build()
      .await?;
    let client = Client {
      inner_client: Arc::new(reopened),
      account_identifier: alix.builder.owner.get_identifier()?.into(),
      app_version: None,
    };
    assert!(!client.is_registered()?);
    assert!(
      xmtp_common::time::timeout(
        std::time::Duration::from_millis(200),
        client.register_identity(
          &handle,
          Some(VisibilityConfirmationOptions {
            timeout_ms: Some(0)
          })
        )
      )
      .await
      .is_err()
    );
    let stored: StoredIdentity = client.inner_client.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, Some(i64::MAX));
    set_registration_cursor_for_test(&client.inner_client.context.db(), receipt);
    // The SDK register path asks for a signature and receives None on reopen.
    assert!(client.create_inbox_signature_request().await?.is_none());
    assert!(client.is_registered()?);
    let stored: StoredIdentity = client.inner_client.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, None);
    client
      .register_identity(
        &handle,
        Some(VisibilityConfirmationOptions {
          timeout_ms: Some(0),
        }),
      )
      .await?;
  }
}
