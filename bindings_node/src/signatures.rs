use crate::client::Client;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use xmtp_id::associations::{
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  verify_signed_with_public_context, AccountId,
};

#[napi]
pub fn verify_signed_with_public_key(
  signature_text: String,
  signature_bytes: Uint8Array,
  public_key: Uint8Array,
) -> Result<()> {
  let signature_bytes = signature_bytes.deref().to_vec();
  let signature_bytes: [u8; 64] = signature_bytes
    .try_into()
    .map_err(|_| Error::from_reason("signature_bytes is not 64 bytes long."))?;

  let public_key = public_key.deref().to_vec();
  let public_key: [u8; 32] = public_key
    .try_into()
    .map_err(|_| Error::from_reason("public_key is not 32 bytes long."))?;

  Ok(
    verify_signed_with_public_context(signature_text, &signature_bytes, &public_key)
      .map_err(ErrorWrapper::from)?,
  )
}

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
  pub async fn add_wallet_signature_text(&self, new_wallet_address: String) -> Result<String> {
    let signature_request = self
      .inner_client()
      .associate_eth_wallet(new_wallet_address.to_lowercase())
      .await
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
      .revoke_eth_wallets(vec![wallet_address.to_lowercase()])
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_all_other_installations_signature_text(&self) -> Result<String> {
    let installation_id = self.inner_client().installation_public_key();
    let inbox_state = self
      .inner_client()
      .inbox_state(true)
      .await
      .map_err(ErrorWrapper::from)?;
    let other_installation_ids = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != installation_id)
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
  pub async fn revoke_installations_signature_text(
    &self,
    installation_ids: Vec<Uint8Array>,
  ) -> Result<String> {
    let installation_ids_bytes: Vec<Vec<u8>> = installation_ids
      .iter()
      .map(|id| id.deref().to_vec())
      .collect();

    let signature_request = self
      .inner_client()
      .revoke_installations(installation_ids_bytes)
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

  #[napi]
  pub fn sign_with_installation_key(&self, signature_text: String) -> Result<Uint8Array> {
    let result = self
      .inner_client()
      .context()
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
