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
use xmtp_mls::storage::StoredConsentRecord;

pub struct FfiConsent {
    pub entity_type: FfiConsentEntityType,
    pub state: FfiConsentState,
    pub entity: String,
}

pub enum FfiConsentState {
    Unknown,
    Allowed,
    Denied,
}

pub enum FfiConsentEntityType {
    ConversationId,
    InboxId,
}

impl From<FfiConsent> for StoredConsentRecord {
    fn from(consent: FfiConsent) -> Self {
        // ... existing code ...
    }
}

impl From<StoredConsentRecord> for FfiConsent {
    fn from(consent: StoredConsentRecord) -> Self {
        // ... existing code ...
    }
}

impl From<FfiConsentEntityType> for ConsentType {
    fn from(entity_type: FfiConsentEntityType) -> Self {
        // ... existing code ...
    }
}

impl From<ConsentState> for FfiConsentState {
    fn from(state: ConsentState) -> Self {
        // ... existing code ...
    }
}

impl From<FfiConsentState> for ConsentState {
    fn from(state: FfiConsentState) -> Self {
        // ... existing code ...
    }
} 