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
use xmtp_mls::api::ApiDebugWrapper;
use xmtp_mls::api::tonic::TonicApiClient;
use xmtp_mls::storage::InboxId;
use xmtp_mls::sync::FfiSyncWorkerMode;
use xmtp_mls::error::GenericError;

pub type RustXmtpClient = MlsClient<ApiDebugWrapper<TonicApiClient>>;

pub struct XmtpApiClient(TonicApiClient);

pub async fn connect_to_backend(
    host: String,
    is_secure: bool,
) -> Result<Arc<XmtpApiClient>, GenericError> {
    // ... existing code ...
}

pub async fn create_client(
    api: Arc<XmtpApiClient>,
    db: Option<String>,
    encryption_key: Option<Vec<u8>>,
    inbox_id: &InboxId,
    account_identifier: FfiIdentifier,
    nonce: u64,
    legacy_signed_private_key_proto: Option<Vec<u8>>,
    device_sync_server_url: Option<String>,
    device_sync_mode: Option<FfiSyncWorkerMode>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    // ... existing code ...
}

pub async fn get_inbox_id_for_identifier(
    api: Arc<XmtpApiClient>,
    account_identifier: FfiIdentifier,
) -> Result<Option<String>, GenericError> {
    // ... existing code ...
}

pub struct FfiXmtpClient {
    inner_client: Arc<RustXmtpClient>,
    #[allow(dead_code)]
    worker: FfiSyncWorker,
    #[allow(dead_code)]
    account_identifier: FfiIdentifier,
}

impl FfiXmtpClient {
    pub fn inbox_id(&self) -> InboxId {
        // ... existing code ...
    }

    pub fn conversations(&self) -> Arc<FfiConversations> {
        // ... existing code ...
    }

    pub fn conversation(&self, conversation_id: Vec<u8>) -> Result<FfiConversation, GenericError> {
        // ... existing code ...
    }

    pub fn dm_conversation(
        &self,
        target_inbox_id: String,
    ) -> Result<FfiConversation, GenericError> {
        // ... existing code ...
    }

    pub fn message(&self, message_id: Vec<u8>) -> Result<FfiMessage, GenericError> {
        // ... existing code ...
    }

    pub async fn can_message(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<HashMap<FfiIdentifier, bool>, GenericError> {
        // ... existing code ...
    }

    pub fn installation_id(&self) -> Vec<u8> {
        // ... existing code ...
    }

    pub fn release_db_connection(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn db_reconnect(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn find_inbox_id(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Option<String>, GenericError> {
        // ... existing code ...
    }

    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<FfiInboxState, GenericError> {
        // ... existing code ...
    }

    pub async fn get_key_package_statuses_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<HashMap<Vec<u8>, FfiKeyPackageStatus>, GenericError> {
        // ... existing code ...
    }

    pub async fn addresses_from_inbox_id(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<String>,
    ) -> Result<Vec<FfiInboxState>, GenericError> {
        // ... existing code ...
    }

    pub async fn get_latest_inbox_state(
        &self,
        inbox_id: String,
    ) -> Result<FfiInboxState, GenericError> {
        // ... existing code ...
    }

    pub async fn set_consent_states(&self, records: Vec<FfiConsent>) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn get_consent_state(
        &self,
        entity_type: FfiConsentEntityType,
        entity: String,
    ) -> Result<FfiConsentState, GenericError> {
        // ... existing code ...
    }

    pub fn sign_with_installation_key(&self, text: &str) -> Result<Vec<u8>, GenericError> {
        // ... existing code ...
    }

    pub fn verify_signed_with_installation_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn verify_signed_with_public_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn sync_preferences(&self) -> Result<u64, GenericError> {
        // ... existing code ...
    }

    pub fn signature_request(&self) -> Option<Arc<FfiSignatureRequest>> {
        // ... existing code ...
    }

    pub async fn register_identity(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn send_sync_request(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn add_identity(
        &self,
        new_identity: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        // ... existing code ...
    }

    pub async fn apply_signature_request(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn revoke_identity(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        // ... existing code ...
    }

    pub async fn revoke_all_other_installations(
        &self,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        // ... existing code ...
    }

    pub async fn revoke_installations(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        // ... existing code ...
    }

    pub async fn change_recovery_identifier(
        &self,
        new_recovery_identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        // ... existing code ...
    }

    pub async fn create_archive(
        &self,
        path: String,
        opts: FfiArchiveOptions,
        key: Vec<u8>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn import_archive(&self, path: String, key: Vec<u8>) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn archive_metadata(
        &self,
        path: String,
        key: Vec<u8>,
    ) -> Result<FfiBackupMetadata, GenericError> {
        // ... existing code ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use xmtp_cryptography::utils::LocalWallet;

    fn tmp_path() -> String {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        path.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn test_legacy_identity() {
        let path = tmp_path();
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let client = create_client(
            connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap(),
            Some(path.clone()),
            None,
            &InboxId::new_random(),
            ident.clone(),
            0,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let real_inbox_id = client.inbox_id();

        let api = connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();

        let from_network = get_inbox_id_for_identifier(api, ident.clone())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(real_inbox_id.to_string(), from_network);
    }

    #[tokio::test]
    async fn test_create_client_with_storage() {
        let path = tmp_path();
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let client_a = create_client(
            connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap(),
            Some(path.clone()),
            None,
            &InboxId::new_random(),
            ident.clone(),
            0,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let client_b = create_client(
            connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap(),
            Some(path),
            None,
            &InboxId::new_random(),
            ident,
            0,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(client_a.inbox_id(), client_b.inbox_id());
    }

    #[tokio::test]
    async fn test_create_client_with_key() {
        let path = tmp_path();
        let wallet = LocalWallet::new_random();
        let wallet_owner = FfiWalletInboxOwner::with_wallet(wallet.clone());
        let ident = wallet_owner.identifier();

        let key = static_enc_key().to_vec();
        let client = create_client(
            connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap(),
            Some(path.clone()),
            Some(key.clone()),
            &InboxId::new_random(),
            ident.clone(),
            0,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let result_errored = create_client(
            connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap(),
            Some(path),
            Some(key),
            &InboxId::new_random(),
            ident,
            0,
            None,
            None,
            None,
        )
        .await;

        assert!(result_errored.is_err());
    }

    #[tokio::test]
    async fn test_can_message() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let can_message = client_a
            .can_message(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        assert!(can_message.get(&client_b.account_identifier).unwrap());
    }

    #[tokio::test]
    async fn test_can_add_wallet_to_inbox() {
        let client_a = new_test_client().await;
        let wallet_b = LocalWallet::new_random();
        let wallet_owner_b = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
        let ident_b = wallet_owner_b.identifier();

        let sig_request = client_a
            .add_identity(ident_b.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet_b)
            .await;

        client_a
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let can_message = client_a
            .can_message(vec![ident_b])
            .await
            .unwrap();

        assert!(can_message.get(&client_a.account_identifier).unwrap());
    }

    #[tokio::test]
    async fn test_can_revoke_wallet() {
        let client_a = new_test_client().await;
        let wallet_b = LocalWallet::new_random();
        let wallet_owner_b = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
        let ident_b = wallet_owner_b.identifier();

        let sig_request = client_a
            .add_identity(ident_b.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet_b)
            .await;

        client_a
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let sig_request = client_a
            .revoke_identity(ident_b.clone())
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&wallet_b)
            .await;

        client_a
            .apply_signature_request(sig_request)
            .await
            .unwrap();

        let can_message = client_a
            .can_message(vec![ident_b])
            .await
            .unwrap();

        assert!(!can_message.get(&client_a.account_identifier).unwrap());
    }

    #[tokio::test]
    async fn test_invalid_external_signature() {
        let client_amal = new_test_client().await;
        let wallet_b = LocalWallet::new_random();
        let wallet_owner_b = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
        let ident_b = wallet_owner_b.identifier();

        let sig_request = client_amal
            .add_identity(ident_b.clone())
            .await
            .unwrap();

        let result = client_amal
            .apply_signature_request(sig_request)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_revoke_all_installations() {
        let client = new_test_client().await;
        let sig_request = client
            .revoke_all_other_installations()
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&client.inner_client.wallet())
            .await;

        client
            .apply_signature_request(sig_request)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_revoke_installations() {
        let client = new_test_client().await;
        let installation_id = client.installation_id();
        let sig_request = client
            .revoke_installations(vec![installation_id])
            .await
            .unwrap();

        sig_request
            .add_wallet_signature(&client.inner_client.wallet())
            .await;

        client
            .apply_signature_request(sig_request)
            .await
            .unwrap();
    }
} 