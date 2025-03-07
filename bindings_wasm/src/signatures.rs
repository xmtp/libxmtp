use crate::{
  client::Client,
  identity::{Identifier, IdentifierKind},
};
use js_sys::Uint8Array;
use std::sync::Arc;
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_id::associations::{
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  AccountId,
};
use xmtp_id::associations::{verify_signed_with_public_context, Identifier as XmtpIdentifier};

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

#[wasm_bindgen]
pub struct PasskeySignature {
  public_key: Vec<u8>,
  signature: Vec<u8>,
  authenticator_data: Vec<u8>,
  client_data_json: String,
}

#[wasm_bindgen]
#[derive(Clone, Eq, Hash, PartialEq)]
pub enum SignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
}

#[wasm_bindgen]
impl Client {
  #[wasm_bindgen(js_name = createInboxSignatureText)]
  pub fn create_inbox_signature_text(&mut self) -> Result<Option<String>, JsError> {
    let signature_request = match self.inner_client().identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(JsError::new("No signature request found")),
    };
    let signature_text = signature_request.signature_text();

    self
      .signature_requests
      .insert(SignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[wasm_bindgen(js_name = addWalletSignatureText)]
  pub async fn add_identifier_signature_text(
    &mut self,
    new_identifier: Identifier,
  ) -> Result<String, JsError> {
    let ident = new_identifier.try_into()?;
    let signature_request = self
      .inner_client()
      .associate_identity(ident)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();

    self
      .signature_requests
      .insert(SignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeWalletSignatureText)]
  pub async fn revoke_identifier_signature_text(
    &mut self,
    identifier: Identifier,
  ) -> Result<String, JsError> {
    let ident = identifier.try_into()?;
    let signature_request = self
      .inner_client()
      .revoke_identities(vec![ident])
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();

    self
      .signature_requests
      .insert(SignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeAllOtherInstallationsSignatureText)]
  pub async fn revoke_all_other_installations_signature_text(&mut self) -> Result<String, JsError> {
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
      .revoke_installations(other_installation_ids)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();

    self
      .signature_requests
      .insert(SignatureRequestType::RevokeInstallations, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeInstallationsSignatureText)]
  pub async fn revoke_installations_signature_text(
    &mut self,
    installation_ids: Vec<Uint8Array>,
  ) -> Result<String, JsError> {
    let installation_ids_bytes: Vec<Vec<u8>> =
      installation_ids.iter().map(|id| id.to_vec()).collect();

    let signature_request = self
      .inner_client()
      .revoke_installations(installation_ids_bytes)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();

    self
      .signature_requests
      .insert(SignatureRequestType::RevokeInstallations, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = addEcdsaSignature)]
  pub async fn add_ecdsa_signature(
    &mut self,
    signature_type: SignatureRequestType,
    signature_bytes: Uint8Array,
  ) -> Result<(), JsError> {
    let verifier = Arc::clone(self.inner_client().scw_verifier());

    if let Some(signature_request) = self.signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.to_vec());
      signature_request
        .add_signature(signature, verifier)
        .await
        .map_err(crate::error)?;
    } else {
      return Err(JsError::new("Signature request not found"));
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = addPasskeySignature)]
  pub async fn add_passkey_signature(
    &mut self,
    signature_type: SignatureRequestType,
    signature: PasskeySignature,
  ) -> Result<(), JsError> {
    let verifier = Arc::clone(self.inner_client().scw_verifier());

    if let Some(signature_request) = self.signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_passkey(
        signature.public_key,
        signature.signature,
        signature.authenticator_data,
        signature.client_data_json,
      );
      signature_request
        .add_signature(signature, verifier)
        .await
        .map_err(crate::error)?;
    } else {
      return Err(JsError::new("Signature request not found"));
    }
    Ok(())
  }

  #[wasm_bindgen(js_name = addScwSignature)]
  pub async fn add_scw_signature(
    &mut self,
    signature_type: SignatureRequestType,
    signature_bytes: Uint8Array,
    chain_id: u64,
    block_number: Option<u64>,
  ) -> Result<(), JsError> {
    let IdentifierKind::Ethereum = self.account_identifier().identifier_kind else {
      return Err(JsError::new(
        "Account identifier must be an ethereum address.",
      ));
    };

    let verifier = Arc::clone(self.inner_client().scw_verifier());
    let ident: XmtpIdentifier = self.account_identifier().clone().try_into()?;
    let address = ident.to_string();

    if let Some(signature_request) = self.signature_requests.get_mut(&signature_type) {
      let account_id = AccountId::new_evm(chain_id, address.clone());
      let signature = NewUnverifiedSmartContractWalletSignature::new(
        signature_bytes.to_vec(),
        account_id,
        block_number,
      );

      signature_request
        .add_new_unverified_smart_contract_signature(signature, &verifier)
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    } else {
      return Err(JsError::new(&format!(
        "Signature request for {address} not found",
      )));
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = applySignatureRequests)]
  pub async fn apply_signature_requests(&mut self) -> Result<(), JsError> {
    let request_types: Vec<SignatureRequestType> =
      self.signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == SignatureRequestType::CreateInbox {
        continue;
      }

      if let Some(signature_request) = self.signature_requests.get(&signature_request_type) {
        self
          .inner_client()
          .apply_signature_request(signature_request.clone())
          .await
          .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

        // remove the signature request after applying it
        self.signature_requests.remove(&signature_request_type);
      }
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = signWithInstallationKey)]
  pub fn sign_with_installation_key(
    &mut self,
    signature_text: String,
  ) -> Result<Uint8Array, JsError> {
    let result = self
      .inner_client()
      .context()
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
