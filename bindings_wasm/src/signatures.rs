use crate::{
  client::Client,
  identity::{Identifier, IdentifierKind},
};
use futures::lock::Mutex;
use js_sys::Uint8Array;
use std::sync::Arc;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::queries::V3Client;
use xmtp_api_grpc::GrpcClient;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::{
  AccountId,
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_id::associations::{Identifier as XmtpIdentifier, verify_signed_with_public_context};
use xmtp_id::scw_verifier::RemoteSignatureVerifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::identity_updates::apply_signature_request_with_verifier;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;

#[wasm_bindgen]
pub struct SignatureRequestHandle {
  inner: Arc<Mutex<SignatureRequest>>,
  scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
}

#[wasm_bindgen(js_name = verifySignedWithPublicKey)]
pub fn verify_signed_with_public_key(
  signature_text: String,
  signature_bytes: Uint8Array,
  public_key: Uint8Array,
) -> Result<(), JsError> {
  let signature_bytes = signature_bytes.to_vec();
  let signature_bytes: [u8; 64] = signature_bytes
    .try_into()
    .map_err(|_| JsError::new("signature_bytes is not 64 bytes long."))?;

  let public_key = public_key.to_vec();
  let public_key: [u8; 32] = public_key
    .try_into()
    .map_err(|_| JsError::new("public_key is not 32 bytes long."))?;

  verify_signed_with_public_context(signature_text, &signature_bytes, &public_key)
    .map_err(|e| JsError::new(format!("{}", e).as_str()))
}

#[wasm_bindgen(js_name = revokeInstallationsSignatureRequest)]
pub async fn revoke_installations_signature_request(
  host: String,
  recovery_identifier: Identifier,
  inbox_id: String,
  installation_ids: Vec<Uint8Array>,
) -> Result<SignatureRequestHandle, JsError> {
  let api_client = V3Client::new(
    GrpcClient::create(&host, true)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?,
  );

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());
  let scw_verifier =
    Arc::new(Box::new(RemoteSignatureVerifier::new(api.clone()))
      as Box<dyn SmartContractSignatureVerifier>);

  let ident = recovery_identifier.try_into()?;
  let ids: Vec<Vec<u8>> = installation_ids.into_iter().map(|i| i.to_vec()).collect();

  let sig_req = revoke_installations_with_verifier(&ident, &inbox_id, ids)
    .await
    .map_err(|e| JsError::new(&e.to_string()))?;

  Ok(SignatureRequestHandle {
    inner: Arc::new(Mutex::new(sig_req)),
    scw_verifier: scw_verifier.clone(),
  })
}

#[wasm_bindgen(js_name = applySignatureRequest)]
pub async fn apply_signature_request(
  host: String,
  signature_request: &SignatureRequestHandle,
) -> Result<(), JsError> {
  let api_client = V3Client::new(
    GrpcClient::create(&host, true)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?,
  );

  let api = ApiClientWrapper::new(Arc::new(api_client), strategies::exponential_cooldown());
  let scw_verifier = Arc::new(RemoteSignatureVerifier::new(api.clone()));

  let inner = signature_request.inner.lock().await;

  apply_signature_request_with_verifier(&api, inner.clone(), &scw_verifier)
    .await
    .map_err(|e| JsError::new(&e.to_string()))?;

  Ok(())
}

#[wasm_bindgen]
pub struct PasskeySignature {
  public_key: Vec<u8>,
  signature: Vec<u8>,
  authenticator_data: Vec<u8>,
  client_data_json: Vec<u8>,
}

/// Methods on SignatureRequestHandle
#[wasm_bindgen]
impl SignatureRequestHandle {
  #[wasm_bindgen(js_name = signatureText)]
  pub async fn signature_text(&self) -> Result<String, JsError> {
    Ok(self.inner.lock().await.signature_text())
  }

  #[wasm_bindgen(js_name = addEcdsaSignature)]
  pub async fn add_ecdsa_signature(&self, signature_bytes: Uint8Array) -> Result<(), JsError> {
    let sig = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.to_vec());
    self
      .inner
      .lock()
      .await
      .add_signature(sig, &self.scw_verifier)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(())
  }

  #[wasm_bindgen(js_name = addPasskeySignature)]
  pub async fn add_passkey_signature(&self, signature: PasskeySignature) -> Result<(), JsError> {
    let sig = UnverifiedSignature::new_passkey(
      signature.public_key,
      signature.signature,
      signature.authenticator_data,
      signature.client_data_json,
    );
    self
      .inner
      .lock()
      .await
      .add_signature(sig, &self.scw_verifier)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(())
  }

  #[wasm_bindgen(js_name = addScwSignature)]
  pub async fn add_scw_signature(
    &self,
    account_identifier: Identifier,
    signature_bytes: Uint8Array,
    chain_id: u64,
    block_number: Option<u64>,
  ) -> Result<(), JsError> {
    if !matches!(account_identifier.identifier_kind, IdentifierKind::Ethereum) {
      return Err(JsError::new("Account identifier must be Ethereum-based"));
    }
    let ident: XmtpIdentifier = account_identifier.try_into()?;
    let account_id = AccountId::new_evm(chain_id, ident.to_string());
    let sig = NewUnverifiedSmartContractWalletSignature::new(
      signature_bytes.to_vec(),
      account_id,
      block_number,
    );
    self
      .inner
      .lock()
      .await
      .add_new_unverified_smart_contract_signature(sig, &self.scw_verifier)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(())
  }
}

#[wasm_bindgen]
impl Client {
  #[wasm_bindgen(js_name = createInboxSignatureRequest)]
  pub fn create_inbox_signature_request(
    &mut self,
  ) -> Result<Option<SignatureRequestHandle>, JsError> {
    let signature_request = match self.inner_client().identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(JsError::new("No signature request found")),
    };

    let handle = SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    };

    Ok(Some(handle))
  }

  #[wasm_bindgen(js_name = addWalletSignatureRequest)]
  pub async fn add_identifier_signature_request(
    &self,
    new_identifier: Identifier,
  ) -> Result<SignatureRequestHandle, JsError> {
    let signature_request = self
      .inner_client()
      .identity_updates()
      .associate_identity(new_identifier.try_into()?)
      .await
      .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[wasm_bindgen(js_name = revokeWalletSignatureRequest)]
  pub async fn revoke_identifier_signature_request(
    &mut self,
    identifier: Identifier,
  ) -> Result<SignatureRequestHandle, JsError> {
    let ident = identifier.try_into()?;
    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_identities(vec![ident])
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[wasm_bindgen(js_name = revokeAllOtherInstallationsSignatureRequest)]
  pub async fn revoke_all_other_installations_signature_request(
    &mut self,
  ) -> Result<SignatureRequestHandle, JsError> {
    let installation_id = self.inner_client().installation_public_key();
    let inbox_state = self
      .inner_client()
      .inbox_state(true)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
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
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[wasm_bindgen(js_name = revokeInstallationsSignatureRequest)]
  pub async fn revoke_installations_signature_request(
    &mut self,
    installation_ids: Vec<Uint8Array>,
  ) -> Result<SignatureRequestHandle, JsError> {
    let installation_ids_bytes: Vec<Vec<u8>> =
      installation_ids.iter().map(|id| id.to_vec()).collect();

    let signature_request = self
      .inner_client()
      .identity_updates()
      .revoke_installations(installation_ids_bytes)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[wasm_bindgen(js_name = changeRecoveryIdentifierSignatureRequest)]
  pub async fn change_recovery_identifier_signature_request(
    &mut self,
    new_recovery_identifier: Identifier,
  ) -> Result<SignatureRequestHandle, JsError> {
    let signature_request = self
      .inner_client()
      .identity_updates()
      .change_recovery_identifier(new_recovery_identifier.try_into()?)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(SignatureRequestHandle {
      inner: Arc::new(Mutex::new(signature_request)),
      scw_verifier: self.inner_client().scw_verifier().clone(),
    })
  }

  #[wasm_bindgen(js_name = applySignatureRequest)]
  pub async fn apply_signature_request(
    &mut self,
    signature_request: &SignatureRequestHandle,
  ) -> Result<(), JsError> {
    let signature_request = signature_request.inner.lock().await;

    self
      .inner_client()
      .identity_updates()
      .apply_signature_request(signature_request.clone())
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = registerIdentity)]
  pub async fn register_identity(
    &mut self,
    signature_request: SignatureRequestHandle,
  ) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    let inner = signature_request.inner.lock().await;

    self
      .inner_client()
      .register_identity(inner.clone())
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = signWithInstallationKey)]
  pub fn sign_with_installation_key(
    &mut self,
    signature_text: String,
  ) -> Result<Uint8Array, JsError> {
    let result = self
      .inner_client()
      .context
      .sign_with_public_context(signature_text)
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(Uint8Array::from(result.as_slice()))
  }

  #[wasm_bindgen(js_name = verifySignedWithInstallationKey)]
  pub fn verify_signed_with_installation_key(
    &mut self,
    signature_text: String,
    signature_bytes: Uint8Array,
  ) -> Result<(), JsError> {
    let public_key = self.inner_client().installation_public_key();
    verify_signed_with_public_key(
      signature_text,
      signature_bytes,
      Uint8Array::from(public_key.as_slice()),
    )
  }
}
