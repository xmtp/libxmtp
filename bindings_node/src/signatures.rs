use crate::client::Client;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use xmtp_id::associations::{
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  AccountId,
};

#[napi]
#[derive(Eq, Hash, PartialEq)]
pub enum SignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
}

#[napi]
impl Client {
  #[napi]
  pub async fn create_inbox_signature_text(&self) -> Result<Option<String>> {
    let signature_request = match self.inner_client().identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(Error::from_reason("No signature request found")),
    };
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::CreateInbox, signature_request);

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

    signature_requests.insert(SignatureRequestType::AddWallet, signature_request);

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

    signature_requests.insert(SignatureRequestType::RevokeWallet, signature_request);

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

    signature_requests.insert(SignatureRequestType::RevokeInstallations, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn add_signature(
    &self,
    signature_type: SignatureRequestType,
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
  pub async fn add_scw_signature(
    &self,
    signature_type: SignatureRequestType,
    signature_bytes: Uint8Array,
    chain_id: BigInt,
    block_number: Option<BigInt>,
  ) -> Result<()> {
    let mut signature_requests = self.signature_requests().lock().await;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let address = self.account_address.clone();
      let account_id = AccountId::new_evm(chain_id.get_u64().1, address);
      let signature = NewUnverifiedSmartContractWalletSignature::new(
        signature_bytes.deref().to_vec(),
        account_id,
        block_number.as_ref().map(|b| b.get_u64().1),
      );

      signature_request
        .add_new_unverified_smart_contract_signature(signature, &self.inner_client().scw_verifier())
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

    let request_types: Vec<SignatureRequestType> = signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == SignatureRequestType::CreateInbox {
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
