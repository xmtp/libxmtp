use crate::ErrorWrapper;
use crate::client::Client;
use crate::identity::Identifier;
use crate::signatures::{SignatureRequestHandle, verify_signed_with_public_key};
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use std::sync::Arc;

#[napi]
impl Client {
  #[napi]
  pub async fn create_inbox_signature_request(&self) -> Result<Option<SignatureRequestHandle>> {
    let Some(signature_request) = self.inner_client().identity().signature_request() else {
      return Ok(None);
    };

    Ok(Some(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    )))
  }

  #[napi]
  pub async fn add_identifier_signature_request(
    &self,
    new_identifier: Identifier,
  ) -> Result<SignatureRequestHandle> {
    let ident = new_identifier.try_into()?;

    let signature_request = self
      .inner_client()
      .identity_updates()
      .associate_identity(ident)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    ))
  }

  #[napi]
  pub async fn revoke_identifier_signature_request(
    &self,
    identifier: Identifier,
  ) -> Result<SignatureRequestHandle> {
    let ident = identifier.try_into()?;

    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_identities(vec![ident])
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    ))
  }

  // Returns Some SignatureRequestHandle if we have installations to revoke.
  // If we have no other installations to revoke, returns None.
  #[napi]
  pub async fn revoke_all_other_installations_signature_request(
    &self,
  ) -> Result<Option<SignatureRequestHandle>> {
    let installation_id = self.inner_client().installation_public_key();

    let inbox_state = self
      .inner_client()
      .inbox_state(true)
      .await
      .map_err(ErrorWrapper::from)?;

    let other_installation_ids: Vec<Vec<u8>> = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != installation_id)
      .collect();

    if other_installation_ids.is_empty() {
      return Ok(None);
    }

    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_installations(other_installation_ids)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(Some(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    )))
  }

  #[napi]
  pub async fn revoke_installations_signature_request(
    &self,
    installation_ids: Vec<Uint8Array>,
  ) -> Result<SignatureRequestHandle> {
    let installation_ids_bytes: Vec<Vec<u8>> = installation_ids
      .iter()
      .map(|id| id.deref().to_vec())
      .collect();

    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_installations(installation_ids_bytes)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    ))
  }

  #[napi]
  pub async fn change_recovery_identifier_signature_request(
    &self,
    new_recovery_identifier: Identifier,
  ) -> Result<SignatureRequestHandle> {
    let signature_request = self
      .inner_client()
      .identity_updates()
      .change_recovery_identifier(new_recovery_identifier.try_into()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(SignatureRequestHandle::new(
      Arc::new(tokio::sync::Mutex::new(signature_request)),
      self.inner_client().scw_verifier().clone(),
    ))
  }

  #[napi]
  pub async fn apply_signature_request(
    &self,
    signature_request: &SignatureRequestHandle,
  ) -> Result<()> {
    let signature_request = signature_request.inner().lock().await;

    self
      .inner_client()
      .identity_updates()
      .apply_signature_request(signature_request.clone())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn sign_with_installation_key(&self, signature_text: String) -> Result<Uint8Array> {
    let result = self
      .inner_client()
      .context
      .sign_with_public_context(signature_text)
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }

  #[napi]
  pub fn verify_signed_with_installation_key(
    &self,
    signature_text: String,
    signature_bytes: Uint8Array,
  ) -> Result<()> {
    let public_key = self.inner_client().installation_public_key();
    verify_signed_with_public_key(signature_text, signature_bytes, public_key.into())
  }
}
