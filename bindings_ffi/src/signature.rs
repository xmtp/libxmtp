use std::sync::{Arc, Mutex};
use xmtp_cryptography::utils::LocalWallet;
use xmtp_mls::{
    client::MlsClient,
    api::ApiDebugWrapper,
    api::tonic::TonicApiClient,
    storage::InboxId,
    identity::FfiIdentifier,
    sync::FfiSyncWorkerMode,
    error::GenericError,
};
use xmtp_api_grpc::GrpcApiClient;
use xmtp_db::Storage;
use xmtp_mls::signature::SignatureRequest;
use xmtp_mls::signature::SmartContractSignatureVerifier;

pub struct FfiSignatureRequest {
    inner: Arc<Mutex<SignatureRequest>>,
    scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
}

pub struct FfiPasskeySignature {
    public_key: Vec<u8>,
    signature: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: Vec<u8>,
}

impl FfiSignatureRequest {
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn add_passkey_signature(
        &self,
        signature: FfiPasskeySignature,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn add_scw_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_id: u64,
        block_number: Option<u64>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn is_ready(&self) -> bool {
        // ... existing code ...
    }

    pub async fn signature_text(&self) -> Result<String, GenericError> {
        // ... existing code ...
    }

    pub async fn missing_address_signatures(&self) -> Result<Vec<String>, GenericError> {
        // ... existing code ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use xmtp_cryptography::utils::LocalWallet;

    #[tokio::test]
    async fn test_add_wallet_signature() {
        let client = new_test_client().await;
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let sig_request = client
            .add_identity(ident.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet)
            .await;

        client
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let can_message = client
            .can_message(vec![ident])
            .await
            .unwrap();

        assert!(can_message.get(&client.account_identifier).unwrap());
    }

    #[tokio::test]
    async fn test_add_wallet_signature_with_invalid_wallet() {
        let client = new_test_client().await;
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let sig_request = client
            .add_identity(ident.clone())
            .await
            .unwrap();

        let result = client
            .apply_signature_request(sig_request)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_revoke_wallet_signature() {
        let client = new_test_client().await;
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let sig_request = client
            .add_identity(ident.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet)
            .await;

        client
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let sig_request = client
            .revoke_identity(ident.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet)
            .await;

        client
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let can_message = client
            .can_message(vec![ident])
            .await
            .unwrap();

        assert!(!can_message.get(&client.account_identifier).unwrap());
    }
} 