use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Elapsed;
use xmtp_cryptography::utils::LocalWallet;
use xmtp_mls::error::GenericError;
use xmtp_mls::identity::FfiIdentifier;
use xmtp_mls::sync::FfiSyncWorkerMode;

pub struct FfiWalletInboxOwner {
    wallet: xmtp_cryptography::utils::LocalWallet,
}

impl FfiWalletInboxOwner {
    pub fn with_wallet(wallet: xmtp_cryptography::utils::LocalWallet) -> Self {
        // ... existing code ...
    }

    pub fn identifier(&self) -> FfiIdentifier {
        // ... existing code ...
    }

    pub fn new() -> Self {
        // ... existing code ...
    }
}

impl FfiInboxOwner for FfiWalletInboxOwner {
    fn get_identifier(&self) -> Result<FfiIdentifier, IdentityValidationError> {
        // ... existing code ...
    }

    fn sign(&self, text: String) -> Result<Vec<u8>, SigningError> {
        // ... existing code ...
    }
}

pub struct RustStreamCallback {
    num_messages: AtomicU32,
    messages: Mutex<Vec<FfiMessage>>,
    conversations: Mutex<Vec<Arc<FfiConversation>>>,
    consent_updates: Mutex<Vec<FfiConsent>>,
    preference_updates: Mutex<Vec<FfiPreferenceUpdate>>,
    notify: Notify,
    inbox_id: Option<String>,
    installation_id: Option<String>,
}

impl RustStreamCallback {
    pub fn message_count(&self) -> u32 {
        // ... existing code ...
    }

    pub fn consent_updates_count(&self) -> usize {
        // ... existing code ...
    }

    pub async fn wait_for_delivery(&self, timeout_secs: Option<u64>) -> Result<(), Elapsed> {
        // ... existing code ...
    }

    pub fn from_client(client: &FfiXmtpClient) -> Self {
        // ... existing code ...
    }
}

impl FfiMessageCallback for RustStreamCallback {
    fn on_message(&self, message: FfiMessage) {
        // ... existing code ...
    }

    fn on_error(&self, error: FfiSubscribeError) {
        // ... existing code ...
    }
}

impl FfiConversationCallback for RustStreamCallback {
    fn on_conversation(&self, group: Arc<FfiConversation>) {
        // ... existing code ...
    }

    fn on_error(&self, error: FfiSubscribeError) {
        // ... existing code ...
    }
}

impl FfiConsentCallback for RustStreamCallback {
    fn on_consent_update(&self, mut consent: Vec<FfiConsent>) {
        // ... existing code ...
    }

    fn on_error(&self, error: FfiSubscribeError) {
        // ... existing code ...
    }
}

impl FfiPreferenceCallback for RustStreamCallback {
    fn on_preference_update(&self, mut preference: Vec<FfiPreferenceUpdate>) {
        // ... existing code ...
    }

    fn on_error(&self, error: FfiSubscribeError) {
        // ... existing code ...
    }
}

pub fn static_enc_key() -> EncryptionKey {
    // ... existing code ...
}

pub async fn register_client_with_wallet(wallet: &FfiWalletInboxOwner, client: &FfiXmtpClient) {
    // ... existing code ...
}

pub async fn register_client_with_wallet_no_panic(
    wallet: &FfiWalletInboxOwner,
    client: &FfiXmtpClient,
) -> Result<(), GenericError> {
    // ... existing code ...
}

pub async fn new_test_client_with_wallet(
    wallet: xmtp_cryptography::utils::LocalWallet,
) -> Arc<FfiXmtpClient> {
    // ... existing code ...
}

pub async fn new_test_client_with_wallet_and_history_sync_url(
    wallet: xmtp_cryptography::utils::LocalWallet,
    history_sync_url: Option<String>,
    sync_worker_mode: Option<FfiSyncWorkerMode>,
) -> Arc<FfiXmtpClient> {
    // ... existing code ...
}

pub async fn new_test_client_no_panic(
    wallet: xmtp_cryptography::utils::LocalWallet,
    sync_server_url: Option<String>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    // ... existing code ...
}

pub async fn new_test_client() -> Arc<FfiXmtpClient> {
    // ... existing code ...
}

pub trait SignWithWallet {
    async fn add_wallet_signature(&self, wallet: &xmtp_cryptography::utils::LocalWallet);
}

impl SignWithWallet for FfiSignatureRequest {
    async fn add_wallet_signature(&self, wallet: &xmtp_cryptography::utils::LocalWallet) {
        // ... existing code ...
    }
} 