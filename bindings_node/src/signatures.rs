use crate::ErrorWrapper;
use crate::client::Client;
use crate::identity::{Identifier, IdentifierKind};
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use std::sync::Arc;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::{
  AccountId,
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  verify_signed_with_public_context,
};
use xmtp_id::scw_verifier::RemoteSignatureVerifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::identity_updates::apply_signature_request_with_verifier;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;

#[napi]
pub struct SignatureRequestHandle {
  inner: Arc<tokio::sync::Mutex<SignatureRequest>>,
  scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
}

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

#[allow(dead_code)]
#[napi]
pub async fn revoke_installations_signature_request(
  host: String,
  recovery_identifier: Identifier,
  inbox_id: String,
  installation_ids: Vec<Uint8Array>,
) -> Result<SignatureRequestHandle> {
  let api_client = TonicApiClient::create(host, true, None::<String>)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());
  let scw_verifier =
    Arc::new(Box::new(RemoteSignatureVerifier::new(api.clone()))
      as Box<dyn SmartContractSignatureVerifier>);

  let ident = recovery_identifier.try_into()?;
  let ids: Vec<Vec<u8>> = installation_ids.into_iter().map(|i| i.to_vec()).collect();

  let signature_request = revoke_installations_with_verifier(&ident, &inbox_id, ids)
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(SignatureRequestHandle {
    inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
    scw_verifier: scw_verifier.clone(),
  })
}

#[allow(dead_code)]
#[napi]
pub async fn apply_signature_request(
  host: String,
  signature_request: &SignatureRequestHandle,
) -> Result<()> {
  let api_client = TonicApiClient::create(host, true, None::<String>)
    .await
    .map_err(ErrorWrapper::from)?;

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());
  let scw_verifier =
    Arc::new(Box::new(RemoteSignatureVerifier::new(api.clone()))
      as Box<dyn SmartContractSignatureVerifier>);

  let inner = signature_request.inner.lock().await;

  apply_signature_request_with_verifier(&api, inner.clone(), &scw_verifier)
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(())
}

#[napi]
impl SignatureRequestHandle {
  pub fn inner(&self) -> &Arc<tokio::sync::Mutex<SignatureRequest>> {
    &self.inner
  }

  #[napi]
  pub async fn signature_text(&self) -> Result<String> {
    Ok(self.inner.lock().await.signature_text())
  }

  #[napi]
  pub async fn add_ecdsa_signature(&self, signature_bytes: Uint8Array) -> Result<()> {
    let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.to_vec());
    let mut inner = self.inner.lock().await;

    inner
      .add_signature(signature, &self.scw_verifier)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_passkey_signature(&self, signature: PasskeySignature) -> Result<()> {
    let new_signature = UnverifiedSignature::new_passkey(
      signature.public_key,
      signature.signature,
      signature.authenticator_data,
      signature.client_data_json,
    );

    let mut inner = self.inner.lock().await;

    inner
      .add_signature(new_signature, &self.scw_verifier)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn add_scw_signature(
    &self,
    account_identifier: Identifier,
    signature_bytes: Uint8Array,
    chain_id: BigInt,
    block_number: Option<BigInt>,
  ) -> Result<()> {
    if !matches!(account_identifier.identifier_kind, IdentifierKind::Ethereum) {
      return Err(Error::from_reason(
        "Account identifier must be Ethereum-based.",
      ));
    }

    let ident: xmtp_id::associations::Identifier = account_identifier.try_into()?;
    let account_id = AccountId::new_evm(chain_id.get_u64().1, ident.to_string());

    let signature = NewUnverifiedSmartContractWalletSignature::new(
      signature_bytes.to_vec(),
      account_id,
      block_number.map(|b| b.get_u64().1),
    );

    let mut inner = self.inner.lock().await;

    inner
      .add_new_unverified_smart_contract_signature(signature, &self.scw_verifier)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
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
  pub async fn create_inbox_signature_request(&self) -> Result<Option<SignatureRequestHandle>> {
    let Some(signature_request) = self.inner_client().identity().signature_request() else {
      return Ok(None);
    };

    let handle = SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    };

    Ok(Some(handle))
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

    Ok(SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
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
    Ok(SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[napi]
  pub async fn revoke_all_other_installations_signature_request(
    &self,
  ) -> Result<SignatureRequestHandle> {
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
    Ok(SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
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
    Ok(SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
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
    Ok(SignatureRequestHandle {
      inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[napi]
  pub async fn apply_signature_request(
    &self,
    signature_request: &SignatureRequestHandle,
  ) -> Result<()> {
    let signature_request = signature_request.inner.lock().await;

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
