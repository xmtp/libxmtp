use crate::mls_client::NapiClient;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use xmtp_id::associations::unverified::UnverifiedSignature;

#[napi]
#[derive(Eq, Hash, PartialEq)]
pub enum NapiSignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
}

#[napi]
impl NapiClient {
  #[napi]
  pub async fn create_inbox_signature_text(&self) -> Result<Option<String>> {
    let signature_request = match self.inner_client().identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(Error::from_reason("No signature request found")),
    };
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(NapiSignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[napi]
  pub async fn add_wallet_signature_text(
    &self,
    existing_wallet_address: String,
    new_wallet_address: String,
  ) -> Result<String> {
    let signature_request = self
      .inner_client()
      .associate_wallet(
        existing_wallet_address.to_lowercase(),
        new_wallet_address.to_lowercase(),
      )
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(NapiSignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_wallet_signature_text(&self, wallet_address: String) -> Result<String> {
    let signature_request = self
      .inner_client()
      .revoke_wallets(vec![wallet_address.to_lowercase()])
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(NapiSignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_installations_signature_text(&self) -> Result<String> {
    let installation_id = self.inner_client().installation_public_key();
    let inbox_state = self
      .inner_client()
      .inbox_state(true)
      .await
      .map_err(ErrorWrapper::from)?;
    let other_installation_ids = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != &installation_id)
      .collect();
    let signature_request = self
      .inner_client()
      .revoke_installations(other_installation_ids)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(
      NapiSignatureRequestType::RevokeInstallations,
      signature_request,
    );

    Ok(signature_text)
  }

  #[napi]
  pub async fn add_signature(
    &self,
    signature_type: NapiSignatureRequestType,
    signature_bytes: Uint8Array,
  ) -> Result<()> {
    let mut signature_requests = self.signature_requests().lock().await;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.deref().to_vec());

      signature_request
        .add_signature(signature, &self.inner_client().scw_verifier())
        .await
        .map_err(ErrorWrapper::from)?;
    } else {
      return Err(Error::from_reason("Signature request not found"));
    }

    Ok(())
  }

  #[napi]
  pub async fn apply_signature_requests(&self) -> Result<()> {
    let mut signature_requests = self.signature_requests().lock().await;

    let request_types: Vec<NapiSignatureRequestType> = signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == NapiSignatureRequestType::CreateInbox {
        continue;
      }

      if let Some(signature_request) = signature_requests.get(&signature_request_type) {
        self
          .inner_client()
          .apply_signature_request(signature_request.clone())
          .await
          .map_err(ErrorWrapper::from)?;

        // remove the signature request after applying it
        signature_requests.remove(&signature_request_type);
      }
    }

    Ok(())
  }
}
