use std::sync::Arc;

use tokio::sync::Mutex;
use xmtp_id::associations::{
    AccountId, AssociationState, SignatureKind as CoreSignatureKind,
    builder::SignatureRequest as CoreSignatureRequest,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};

use crate::{
    InboxID, InstallationID, PublicIdentity, Signature, Signer, SigningRequest, Timestamp,
    XmtpError, client::CoreClient, signer,
};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum SignatureKind {
    Erc191,
    Erc1271,
    InstallationKey,
    LegacyDelegated,
    P256,
}

impl From<CoreSignatureKind> for SignatureKind {
    fn from(value: CoreSignatureKind) -> Self {
        match value {
            CoreSignatureKind::Erc191 => Self::Erc191,
            CoreSignatureKind::Erc1271 => Self::Erc1271,
            CoreSignatureKind::InstallationKey => Self::InstallationKey,
            CoreSignatureKind::LegacyDelegated => Self::LegacyDelegated,
            CoreSignatureKind::P256 => Self::P256,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Installation {
    pub id: InstallationID,
    pub created_at_ns: Option<Timestamp>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InboxState {
    pub inbox_id: InboxID,
    pub identities: Vec<PublicIdentity>,
    pub installations: Vec<Installation>,
    pub recovery_identity: PublicIdentity,
    pub creation_signature_kind: Option<SignatureKind>,
}

impl InboxState {
    pub(crate) fn from_core(
        state: AssociationState,
        creation_signature_kind: Option<CoreSignatureKind>,
    ) -> Result<Self, XmtpError> {
        let installations = state
            .installations()
            .into_iter()
            .map(|installation| {
                Ok(Installation {
                    id: InstallationID::try_from(hex::encode(installation.id))?,
                    created_at_ns: installation
                        .client_timestamp_ns
                        .map(|ns| Timestamp(i64::try_from(ns).unwrap_or(i64::MAX))),
                })
            })
            .collect::<Result<Vec<_>, XmtpError>>()?;
        Ok(Self {
            inbox_id: InboxID::try_from(state.inbox_id().to_owned())?,
            identities: state.identifiers().into_iter().map(Into::into).collect(),
            installations,
            recovery_identity: state.recovery_identifier().clone().into(),
            creation_signature_kind: creation_signature_kind.map(Into::into),
        })
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InboxCountEntry {
    pub inbox_id: InboxID,
    pub count: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CanMessageEntry {
    pub identity: PublicIdentity,
    pub can_message: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct KeyPackageLifetime {
    pub not_before: u64,
    pub not_after: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct KeyPackageStatus {
    pub lifetime: Option<KeyPackageLifetime>,
    pub validation_error: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct KeyPackageStatusEntry {
    pub installation_id: InstallationID,
    pub status: KeyPackageStatus,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CatchUpSummary {
    pub messages: u64,
    pub conversations: u64,
    pub failed: u64,
    pub completed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<xmtp_mls::subscriptions::catch_up::CatchUpSummary> for CatchUpSummary {
    fn from(value: xmtp_mls::subscriptions::catch_up::CatchUpSummary) -> Self {
        Self {
            messages: value.messages,
            conversations: value.conversations,
            failed: value.failed,
            completed: value.completed,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupSyncSummary {
    pub eligible: u64,
    pub synced: u64,
}

impl From<xmtp_mls::groups::welcome_sync::GroupSyncSummary> for GroupSyncSummary {
    fn from(value: xmtp_mls::groups::welcome_sync::GroupSyncSummary) -> Self {
        Self {
            eligible: value.num_eligible as u64,
            synced: value.num_synced as u64,
        }
    }
}

/// A request for an identity update. The caller adds signatures before applying it.
#[derive(uniffi::Object)]
pub struct SignatureRequest {
    inner: Mutex<CoreSignatureRequest>,
    client: Arc<CoreClient>,
}

impl SignatureRequest {
    pub(crate) fn new(inner: CoreSignatureRequest, client: Arc<CoreClient>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(inner),
            client,
        })
    }

    pub(crate) async fn clone_inner(&self) -> CoreSignatureRequest {
        self.inner.lock().await.clone()
    }

    pub(crate) fn belongs_to(&self, client: &Arc<CoreClient>) -> bool {
        Arc::ptr_eq(&self.client, client)
    }
}

#[xmtp_macro::sdk_export]
impl SignatureRequest {
    pub async fn signature_text(&self) -> String {
        self.inner.lock().await.signature_text()
    }

    pub async fn sign(&self, signer: Arc<dyn Signer>) -> Result<(), XmtpError> {
        let text = self.signature_text().await;
        let signature = signer::sign(signer, SigningRequest { text }).await?;
        self.add_signature(signature).await
    }

    pub async fn add_signature(&self, signature: Signature) -> Result<(), XmtpError> {
        let mut request = self.inner.lock().await;
        let verifier = self.client.scw_verifier();
        match signature {
            Signature::Ecdsa(bytes) => request
                .add_signature(UnverifiedSignature::new_recoverable_ecdsa(bytes), &verifier)
                .await
                .map_err(XmtpError::from_signature_request),
            Signature::Passkey {
                signature,
                public_key,
                authenticator_data,
                client_data_json,
            } => request
                .add_signature(
                    UnverifiedSignature::new_passkey(
                        public_key,
                        signature,
                        authenticator_data,
                        client_data_json,
                    ),
                    &verifier,
                )
                .await
                .map_err(XmtpError::from_signature_request),
            Signature::Scw {
                bytes,
                address,
                chain_id,
                block_number,
            } => request
                .add_new_unverified_smart_contract_signature(
                    NewUnverifiedSmartContractWalletSignature::new(
                        bytes,
                        AccountId::new_evm(chain_id, address),
                        block_number,
                    ),
                    &verifier,
                )
                .await
                .map_err(XmtpError::from_signature_request),
        }
    }
}
