use crate::client::Client;
use crate::identity::{Identifier, IdentifierKind};
use crate::ErrorWrapper;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::Mutex;
use xmtp_api::{strategies, ApiClientWrapper};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_id::associations::{
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  verify_signed_with_public_context, AccountId,
};
use xmtp_id::scw_verifier::RemoteSignatureVerifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::identity_updates::apply_signature_request_with_verifier;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;

static SIGNATURE_REQUESTS: Lazy<Mutex<HashMap<SignatureRequestType, SignatureRequest>>> =
  Lazy::new(|| Mutex::new(HashMap::new()));

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
pub async fn revoke_installations_signature_text(
  recovery_identifier: Identifier,
  inbox_id: String,
  installation_ids: Vec<Uint8Array>,
) -> Result<String> {
  let ident = recovery_identifier.try_into()?;

  let ids: Vec<Vec<u8>> = installation_ids.into_iter().map(|i| i.to_vec()).collect();

  let signature_request = revoke_installations_with_verifier(&ident, &inbox_id, ids)
    .await
    .map_err(|e| Error::from_reason(format!("Failed to revoke: {}", e)))?;

  let signature_text = signature_request.signature_text();

  // Save it in the global map
  let mut map = SIGNATURE_REQUESTS.lock().await;
  map.insert(SignatureRequestType::RevokeInstallations, signature_request);

  Ok(signature_text)
}

#[napi]
pub async fn apply_signature_request(
  host: String,
  signature_type: SignatureRequestType,
) -> Result<()> {
  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());
  let scw_verifier =
    Arc::new(Box::new(RemoteSignatureVerifier::new(api.clone()))
      as Box<dyn SmartContractSignatureVerifier>);

  let mut map = SIGNATURE_REQUESTS.lock().await;
  let Some(signature_request) = map.remove(&signature_type) else {
    return Err(Error::from_reason("No signature request found"));
  };

  apply_signature_request_with_verifier(&api, signature_request, &scw_verifier)
    .await
    .map_err(|e| Error::from_reason(format!("Failed to apply signature: {}", e)))?;

  Ok(())
}

#[napi]
pub async fn add_ecdsa_signature(
  host: String,
  signature_type: SignatureRequestType,
  signature_bytes: Uint8Array,
) -> Result<()> {
  let mut map = SIGNATURE_REQUESTS.lock().await;
  let Some(signature_request) = map.get_mut(&signature_type) else {
    return Err(Error::from_reason("Signature request not found"));
  };

  let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.deref().to_vec());

  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());

  let verifier = Arc::new(
    Box::new(RemoteSignatureVerifier::new(api)) as Box<dyn SmartContractSignatureVerifier>
  );
  signature_request
    .add_signature(signature, &verifier.clone())
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(())
}

#[napi]
pub async fn add_passkey_signature(
  host: String,
  signature_type: SignatureRequestType,
  signature: PasskeySignature,
) -> Result<()> {
  let mut map = SIGNATURE_REQUESTS.lock().await;
  let Some(signature_request) = map.get_mut(&signature_type) else {
    return Err(Error::from_reason("Signature request not found"));
  };

  let signature = UnverifiedSignature::new_passkey(
    signature.public_key,
    signature.signature,
    signature.authenticator_data,
    signature.client_data_json,
  );

  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());

  let verifier = Arc::new(
    Box::new(RemoteSignatureVerifier::new(api)) as Box<dyn SmartContractSignatureVerifier>
  );
  signature_request
    .add_signature(signature, &verifier.clone())
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(())
}

#[napi]
pub async fn add_scw_signature(
  host: String,
  signature_type: SignatureRequestType,
  account_identifier: Identifier,
  signature_bytes: Uint8Array,
  chain_id: BigInt,
  block_number: Option<BigInt>,
) -> Result<()> {
  if !matches!(account_identifier.identifier_kind, IdentifierKind::Ethereum) {
    return Err(Error::from_reason("Identifier must be Ethereum-based."));
  }

  let ident: XmtpIdentifier = account_identifier.try_into()?;
  let account_id = AccountId::new_evm(chain_id.get_u64().1, ident.to_string());

  let signature = NewUnverifiedSmartContractWalletSignature::new(
    signature_bytes.deref().to_vec(),
    account_id,
    block_number.map(|b| b.get_u64().1),
  );

  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());

  let verifier = Arc::new(
    Box::new(RemoteSignatureVerifier::new(api)) as Box<dyn SmartContractSignatureVerifier>
  );
  let mut map = SIGNATURE_REQUESTS.lock().await;
  let Some(signature_request) = map.get_mut(&signature_type) else {
    return Err(Error::from_reason("Signature request not found"));
  };

  signature_request
    .add_new_unverified_smart_contract_signature(signature, &verifier.clone())
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(())
}

#[napi]
#[derive(Eq, Hash, PartialEq)]
pub enum SignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
  ChangeRecoveryIdentifier,
}

#[napi(object)]
pub struct PasskeySignature {
  pub public_key: Vec<u8>,
  pub signature: Vec<u8>,
  pub authenticator_data: Vec<u8>,
  pub client_data_json: Vec<u8>,
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
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

    signature_requests.insert(SignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[napi]
  pub async fn add_identifier_signature_text(&self, new_identifier: Identifier) -> Result<String> {
    let ident = new_identifier.try_into()?;

    let signature_request = self
      .inner_client()
      .identity_updates()
      .associate_identity(ident)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

    signature_requests.insert(SignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_identifier_signature_text(&self, identifier: Identifier) -> Result<String> {
    let ident = identifier.try_into()?;

    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_identities(vec![ident])
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

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
      .identity_updates()
      .revoke_installations(other_installation_ids)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

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
      .identity_updates()
      .revoke_installations(installation_ids_bytes)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

    signature_requests.insert(SignatureRequestType::RevokeInstallations, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn change_recovery_identifier_signature_text(
    &self,
    new_recovery_identifier: Identifier,
  ) -> Result<String> {
    let signature_request = self
      .inner_client()
      .identity_updates()
      .change_recovery_identifier(new_recovery_identifier.try_into()?)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

    signature_requests.insert(
      SignatureRequestType::ChangeRecoveryIdentifier,
      signature_request,
    );

    Ok(signature_text)
  }

  #[napi]
  pub async fn apply_signature_requests(&self) -> Result<()> {
    let mut signature_requests = SIGNATURE_REQUESTS.lock().await;

    let request_types: Vec<SignatureRequestType> = signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == SignatureRequestType::CreateInbox {
        continue;
      }

      if let Some(signature_request) = signature_requests.get(&signature_request_type) {
        self
          .inner_client()
          .identity_updates()
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
