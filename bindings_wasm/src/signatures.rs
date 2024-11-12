use js_sys::Uint8Array;
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_id::associations::{
  unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
  AccountId,
};

use crate::client::Client;

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
  pub async fn create_inbox_signature_text(&self) -> Result<Option<String>, JsError> {
    let signature_request = match self.inner_client().identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(JsError::new("No signature request found")),
    };
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[wasm_bindgen(js_name = addWalletSignatureText)]
  pub async fn add_wallet_signature_text(
    &self,
    existing_wallet_address: String,
    new_wallet_address: String,
  ) -> Result<String, JsError> {
    let signature_request = self
      .inner_client()
      .associate_wallet(
        existing_wallet_address.to_lowercase(),
        new_wallet_address.to_lowercase(),
      )
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeWalletSignatureText)]
  pub async fn revoke_wallet_signature_text(
    &self,
    wallet_address: String,
  ) -> Result<String, JsError> {
    let signature_request = self
      .inner_client()
      .revoke_wallets(vec![wallet_address.to_lowercase()])
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeInstallationsSignatureText)]
  pub async fn revoke_installations_signature_text(&self) -> Result<String, JsError> {
    let installation_id = self.inner_client().installation_public_key();
    let inbox_state = self
      .inner_client()
      .inbox_state(true)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let other_installation_ids = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != &installation_id)
      .collect();
    let signature_request = self
      .inner_client()
      .revoke_installations(other_installation_ids)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests().lock().await;

    signature_requests.insert(SignatureRequestType::RevokeInstallations, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = addSignature)]
  pub async fn add_signature(
    &self,
    signature_type: SignatureRequestType,
    signature_bytes: Uint8Array,
  ) -> Result<(), JsError> {
    let mut signature_requests = self.signature_requests().lock().await;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.to_vec());

      signature_request
        .add_signature(signature, self.inner_client().scw_verifier())
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    } else {
      return Err(JsError::new("Signature request not found"));
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = addScwSignature)]
  pub async fn add_scw_signature(
    &self,
    signature_type: SignatureRequestType,
    signature_bytes: Uint8Array,
    chain_id: u64,
    block_number: Option<u64>,
  ) -> Result<(), JsError> {
    let mut signature_requests = self.signature_requests().lock().await;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let address = self.account_address();
      let account_id = AccountId::new_evm(chain_id, address);
      let signature = NewUnverifiedSmartContractWalletSignature::new(
        signature_bytes.to_vec(),
        account_id,
        block_number,
      );

      signature_request
        .add_new_unverified_smart_contract_signature(signature, &self.inner_client().scw_verifier())
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    } else {
      return Err(JsError::new("Signature request not found"));
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = applySignatureRequests)]
  pub async fn apply_signature_requests(&self) -> Result<(), JsError> {
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
          .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

        // remove the signature request after applying it
        signature_requests.remove(&signature_request_type);
      }
    }

    Ok(())
  }
}
