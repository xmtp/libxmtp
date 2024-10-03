pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::logger::FfiLogger;
use crate::GenericError;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::{sync::Mutex, task::AbortHandle};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::AccountId;
use xmtp_id::associations::AssociationState;
use xmtp_id::associations::MemberIdentifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    associations::{builder::SignatureRequest, generate_inbox_id as xmtp_id_generate_inbox_id},
    InboxId,
};
use xmtp_mls::client::FindGroupParams;
use xmtp_mls::groups::group_mutable_metadata::MetadataField;
use xmtp_mls::groups::group_permissions::BasePolicies;
use xmtp_mls::groups::group_permissions::GroupMutablePermissionsError;
use xmtp_mls::groups::group_permissions::MembershipPolicies;
use xmtp_mls::groups::group_permissions::MetadataBasePolicies;
use xmtp_mls::groups::group_permissions::MetadataPolicies;
use xmtp_mls::groups::group_permissions::PermissionsBasePolicies;
use xmtp_mls::groups::group_permissions::PermissionsPolicies;
use xmtp_mls::groups::group_permissions::PolicySet;
use xmtp_mls::groups::intents::PermissionPolicyOption;
use xmtp_mls::groups::intents::PermissionUpdateType;
use xmtp_mls::groups::GroupMetadataOptions;
use xmtp_mls::storage::consent_record::ConsentState;
use xmtp_mls::storage::consent_record::ConsentType;
use xmtp_mls::storage::consent_record::StoredConsentRecord;
use xmtp_mls::{
    api::ApiClientWrapper,
    builder::ClientBuilder,
    client::Client as MlsClient,
    client::ClientError,
    groups::{
        group_metadata::{ConversationType, GroupMetadata},
        group_permissions::GroupMutablePermissions,
        members::PermissionLevel,
        MlsGroup, PreconfiguredPolicies, UpdateAdminListType,
    },
    identity::IdentityStrategy,
    retry::Retry,
    storage::{
        group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
        EncryptedMessageStore, EncryptionKey, StorageOption,
    },
    subscriptions::StreamHandle,
};

pub type RustXmtpClient = MlsClient<TonicApiClient>;

/// It returns a new client of the specified `inbox_id`.
/// Note that the `inbox_id` must be either brand new or already associated with the `account_address`.
/// i.e. `inbox_id` cannot be associated with another account address.
///
/// Prior to calling this function, it's suggested to form `inbox_id`, `account_address`, and `nonce` like below.
///
/// ```text
/// inbox_id = get_inbox_id_for_address(account_address)
/// nonce = 0
///
/// // if inbox_id is not associated, we will create new one.
/// if !inbox_id {
///     if !legacy_key { nonce = random_u64() }
///     inbox_id = generate_inbox_id(account_address, nonce)
/// } // Otherwise, we will just use the inbox and ignore the nonce.
/// db_path = $inbox_id-$env
///
/// xmtp.create_client(account_address, nonce, inbox_id, Option<legacy_signed_private_key_proto>)
/// ```
#[allow(clippy::too_many_arguments)]
#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    logger: Box<dyn FfiLogger>,
    host: String,
    is_secure: bool,
    db: Option<String>,
    encryption_key: Option<Vec<u8>>,
    inbox_id: &InboxId,
    account_address: String,
    nonce: u64,
    legacy_signed_private_key_proto: Option<Vec<u8>>,
    history_sync_url: Option<String>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    init_logger(logger);
    log::info!(
        "Creating API client for host: {}, isSecure: {}",
        host,
        is_secure
    );
    let api_client = TonicApiClient::create(host.clone(), is_secure).await?;

    log::info!(
        "Creating message store with path: {:?} and encryption key: {} of length {:?}",
        db,
        encryption_key.is_some(),
        encryption_key.as_ref().map(|k| k.len())
    );

    let storage_option = match db {
        Some(path) => StorageOption::Persistent(path),
        None => StorageOption::Ephemeral,
    };

    let store = match encryption_key {
        Some(key) => {
            let key: EncryptionKey = key
                .try_into()
                .map_err(|_| "Malformed 32 byte encryption key".to_string())?;
            EncryptedMessageStore::new(storage_option, key)?
        }
        None => EncryptedMessageStore::new_unencrypted(storage_option)?,
    };
    log::info!("Creating XMTP client");
    let identity_strategy = IdentityStrategy::CreateIfNotFound(
        inbox_id.clone(),
        account_address.clone(),
        nonce,
        legacy_signed_private_key_proto,
    );

    let xmtp_client: RustXmtpClient = match history_sync_url {
        Some(url) => {
            ClientBuilder::new(identity_strategy)
                .api_client(api_client)
                .store(store)
                .history_sync_url(&url)
                .build()
                .await?
        }
        None => {
            ClientBuilder::new(identity_strategy)
                .api_client(api_client)
                .store(store)
                .build()
                .await?
        }
    };

    log::info!(
        "Created XMTP client for inbox_id: {}",
        xmtp_client.inbox_id()
    );
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Arc::new(xmtp_client),
        account_address,
    }))
}

#[allow(unused)]
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_inbox_id_for_address(
    logger: Box<dyn FfiLogger>,
    host: String,
    is_secure: bool,
    account_address: String,
) -> Result<Option<String>, GenericError> {
    let api_client = ApiClientWrapper::new(
        TonicApiClient::create(host.clone(), is_secure).await?,
        Retry::default(),
    );

    let results = api_client
        .get_inbox_ids(vec![account_address.clone()])
        .await
        .map_err(GenericError::from_error)?;

    Ok(results.get(&account_address).cloned())
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(account_address: String, nonce: u64) -> String {
    xmtp_id_generate_inbox_id(&account_address, &nonce)
}

#[derive(uniffi::Object)]
pub struct FfiSignatureRequest {
    inner: Arc<Mutex<SignatureRequest>>,
    scw_verifier: Box<dyn SmartContractSignatureVerifier>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSignatureRequest {
    // Signature that's signed by EOA wallet
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        inner
            .add_signature(
                UnverifiedSignature::new_recoverable_ecdsa(signature_bytes),
                self.scw_verifier.clone().as_ref(),
            )
            .await?;

        Ok(())
    }

    // Signature that's signed by smart contract wallet
    pub async fn add_scw_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_id: u64,
        block_number: Option<u64>,
    ) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        let account_id = AccountId::new_evm(chain_id, address);

        let block_number = match block_number {
            Some(bn) => bn,
            None => {
                self.scw_verifier
                    .current_block_number(&chain_id.to_string())
                    .await
                    .map_err(GenericError::Verifier)?
                    .0[0]
            }
        };

        let signature = UnverifiedSignature::new_smart_contract_wallet(
            signature_bytes,
            account_id,
            block_number,
        );
        inner
            .add_signature(signature, self.scw_verifier.clone().as_ref())
            .await?;
        Ok(())
    }

    pub async fn is_ready(&self) -> bool {
        self.inner.lock().await.is_ready()
    }

    pub async fn signature_text(&self) -> Result<String, GenericError> {
        Ok(self.inner.lock().await.signature_text())
    }

    /// missing signatures that are from [MemberKind::Address]
    pub async fn missing_address_signatures(&self) -> Result<Vec<String>, GenericError> {
        let inner = self.inner.lock().await;
        Ok(inner
            .missing_address_signatures()
            .iter()
            .map(|member| member.to_string())
            .collect())
    }
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: Arc<RustXmtpClient>,
    #[allow(dead_code)]
    account_address: String,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn inbox_id(&self) -> InboxId {
        self.inner_client.inbox_id()
    }

    pub fn conversations(&self) -> Arc<FfiConversations> {
        Arc::new(FfiConversations {
            inner_client: self.inner_client.clone(),
        })
    }

    pub fn group(&self, group_id: Vec<u8>) -> Result<FfiGroup, GenericError> {
        let convo = self.inner_client.group(group_id)?;
        Ok(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: convo.group_id,
            created_at_ns: convo.created_at_ns,
        })
    }

    pub fn message(&self, message_id: Vec<u8>) -> Result<FfiMessage, GenericError> {
        let message = self.inner_client.message(message_id)?;
        Ok(message.into())
    }

    pub async fn can_message(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<HashMap<String, bool>, GenericError> {
        let inner = self.inner_client.as_ref();

        let results: HashMap<String, bool> = inner.can_message(account_addresses).await?;

        Ok(results)
    }

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner_client.installation_public_key()
    }

    pub fn release_db_connection(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.release_db_connection()?)
    }

    pub async fn db_reconnect(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.reconnect_db()?)
    }

    pub async fn find_inbox_id(&self, address: String) -> Result<Option<String>, GenericError> {
        let inner = self.inner_client.as_ref();

        let result = inner.find_inbox_id_from_address(address).await?;
        Ok(result)
    }

    /**
     * Get the client's inbox state.
     *
     * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
     * Otherwise, the state will be read from the local database.
     */
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<FfiInboxState, GenericError> {
        let state = self.inner_client.inbox_state(refresh_from_network).await?;
        Ok(state.into())
    }

    pub async fn get_latest_inbox_state(
        &self,
        inbox_id: String,
    ) -> Result<FfiInboxState, GenericError> {
        let state = self
            .inner_client
            .get_latest_association_state(&self.inner_client.store().conn()?, &inbox_id)
            .await?;
        Ok(state.into())
    }

    pub async fn set_consent_states(&self, records: Vec<FfiConsent>) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        let stored_records: Vec<StoredConsentRecord> =
            records.into_iter().map(StoredConsentRecord::from).collect();

        inner.set_consent_states(stored_records).await?;
        Ok(())
    }

    pub async fn get_consent_state(
        &self,
        entity_type: FfiConsentEntityType,
        entity: String,
    ) -> Result<FfiConsentState, GenericError> {
        let inner = self.inner_client.as_ref();
        let result = inner.get_consent_state(entity_type.into(), entity).await?;

        Ok(result.into())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn signature_request(&self) -> Option<Arc<FfiSignatureRequest>> {
        let scw_verifier = self.inner_client.context().scw_verifier.clone();
        self.inner_client
            .identity()
            .signature_request()
            .map(|request| {
                Arc::new(FfiSignatureRequest {
                    inner: Arc::new(Mutex::new(request)),
                    scw_verifier,
                })
            })
    }

    pub async fn register_identity(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        let signature_request = signature_request.inner.lock().await;
        self.inner_client
            .register_identity(signature_request.clone())
            .await?;

        Ok(())
    }

    pub async fn request_history_sync(&self) -> Result<(), GenericError> {
        self.inner_client
            .send_history_request()
            .await
            .map_err(GenericError::from_error)?;
        Ok(())
    }

    /// Adds an identity - really a wallet address - to the existing client
    pub async fn add_wallet(
        &self,
        existing_wallet_address: &str,
        new_wallet_address: &str,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        let signature_request = self
            .inner_client
            .associate_wallet(existing_wallet_address.into(), new_wallet_address.into())?;

        let request = Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.context().scw_verifier.clone(),
        });

        Ok(request)
    }

    pub async fn apply_signature_request(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        let signature_request = signature_request.inner.lock().await;
        self.inner_client
            .apply_signature_request(signature_request.clone())
            .await?;

        Ok(())
    }

    /// Revokes or removes an identity - really a wallet address - from the existing client
    pub async fn revoke_wallet(
        &self,
        wallet_address: &str,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        let signature_request = self
            .inner_client
            .revoke_wallets(vec![wallet_address.into()])
            .await?;

        let request = Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.context().scw_verifier.clone(),
        });

        Ok(request)
    }

    /**
     * Revokes all installations except the one the client is currently using
     */
    pub async fn revoke_all_other_installations(
        &self,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        let installation_id = self.inner_client.installation_public_key();
        let inbox_state = self.inner_client.inbox_state(true).await?;
        let other_installation_ids = inbox_state
            .installation_ids()
            .into_iter()
            .filter(|id| id != &installation_id)
            .collect();

        let signature_request = self
            .inner_client
            .revoke_installations(other_installation_ids)
            .await?;

        Ok(Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.context().scw_verifier.clone(),
        }))
    }
}

#[derive(uniffi::Record)]
pub struct FfiInboxState {
    pub inbox_id: String,
    pub recovery_address: String,
    pub installations: Vec<FfiInstallation>,
    pub account_addresses: Vec<String>,
}

#[derive(uniffi::Record)]
pub struct FfiInstallation {
    pub id: Vec<u8>,
    pub client_timestamp_ns: Option<u64>,
}

impl From<AssociationState> for FfiInboxState {
    fn from(state: AssociationState) -> Self {
        Self {
            inbox_id: state.inbox_id().to_string(),
            recovery_address: state.recovery_address().to_string(),
            installations: state
                .members()
                .into_iter()
                .filter_map(|m| match m.identifier {
                    MemberIdentifier::Address(_) => None,
                    MemberIdentifier::Installation(inst) => Some(FfiInstallation {
                        id: inst,
                        client_timestamp_ns: m.client_timestamp_ns,
                    }),
                })
                .collect(),
            account_addresses: state.account_addresses(),
        }
    }
}

#[derive(uniffi::Record, Default)]
pub struct FfiListConversationsOptions {
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(uniffi::Object)]
pub struct FfiConversations {
    inner_client: Arc<RustXmtpClient>,
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiGroupPermissionsOptions {
    AllMembers,
    AdminOnly,
    CustomPolicy,
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiPermissionUpdateType {
    AddMember,
    RemoveMember,
    AddAdmin,
    RemoveAdmin,
    UpdateMetadata,
}

impl From<&FfiPermissionUpdateType> for PermissionUpdateType {
    fn from(update_type: &FfiPermissionUpdateType) -> Self {
        match update_type {
            FfiPermissionUpdateType::AddMember => PermissionUpdateType::AddMember,
            FfiPermissionUpdateType::RemoveMember => PermissionUpdateType::RemoveMember,
            FfiPermissionUpdateType::AddAdmin => PermissionUpdateType::AddAdmin,
            FfiPermissionUpdateType::RemoveAdmin => PermissionUpdateType::RemoveAdmin,
            FfiPermissionUpdateType::UpdateMetadata => PermissionUpdateType::UpdateMetadata,
        }
    }
}

#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum FfiPermissionPolicy {
    Allow,
    Deny,
    Admin,
    SuperAdmin,
    DoesNotExist,
    Other,
}

impl TryInto<PermissionPolicyOption> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<PermissionPolicyOption, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
            FfiPermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
            FfiPermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
            FfiPermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<MembershipPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<MembershipPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(MembershipPolicies::allow()),
            FfiPermissionPolicy::Deny => Ok(MembershipPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(MembershipPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => Ok(MembershipPolicies::allow_if_actor_super_admin()),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<MetadataPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<MetadataPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(MetadataPolicies::allow()),
            FfiPermissionPolicy::Deny => Ok(MetadataPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(MetadataPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => Ok(MetadataPolicies::allow_if_actor_super_admin()),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<PermissionsPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<PermissionsPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Deny => Ok(PermissionsPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(PermissionsPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => {
                Ok(PermissionsPolicies::allow_if_actor_super_admin())
            }
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl From<&MembershipPolicies> for FfiPermissionPolicy {
    fn from(policies: &MembershipPolicies) -> Self {
        if let MembershipPolicies::Standard(base_policy) = policies {
            match base_policy {
                BasePolicies::Allow => FfiPermissionPolicy::Allow,
                BasePolicies::Deny => FfiPermissionPolicy::Deny,
                BasePolicies::AllowSameMember => FfiPermissionPolicy::Other,
                BasePolicies::AllowIfAdminOrSuperAdmin => FfiPermissionPolicy::Admin,
                BasePolicies::AllowIfSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

impl From<&MetadataPolicies> for FfiPermissionPolicy {
    fn from(policies: &MetadataPolicies) -> Self {
        if let MetadataPolicies::Standard(base_policy) = policies {
            match base_policy {
                MetadataBasePolicies::Allow => FfiPermissionPolicy::Allow,
                MetadataBasePolicies::Deny => FfiPermissionPolicy::Deny,
                MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => FfiPermissionPolicy::Admin,
                MetadataBasePolicies::AllowIfActorSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

impl From<&PermissionsPolicies> for FfiPermissionPolicy {
    fn from(policies: &PermissionsPolicies) -> Self {
        if let PermissionsPolicies::Standard(base_policy) = policies {
            match base_policy {
                PermissionsBasePolicies::Deny => FfiPermissionPolicy::Deny,
                PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                    FfiPermissionPolicy::Admin
                }
                PermissionsBasePolicies::AllowIfActorSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct FfiPermissionPolicySet {
    pub add_member_policy: FfiPermissionPolicy,
    pub remove_member_policy: FfiPermissionPolicy,
    pub add_admin_policy: FfiPermissionPolicy,
    pub remove_admin_policy: FfiPermissionPolicy,
    pub update_group_name_policy: FfiPermissionPolicy,
    pub update_group_description_policy: FfiPermissionPolicy,
    pub update_group_image_url_square_policy: FfiPermissionPolicy,
    pub update_group_pinned_frame_url_policy: FfiPermissionPolicy,
}

impl From<PreconfiguredPolicies> for FfiGroupPermissionsOptions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::AllMembers => FfiGroupPermissionsOptions::AllMembers,
            PreconfiguredPolicies::AdminsOnly => FfiGroupPermissionsOptions::AdminOnly,
        }
    }
}

impl TryFrom<FfiPermissionPolicySet> for PolicySet {
    type Error = GroupMutablePermissionsError;
    fn try_from(policy_set: FfiPermissionPolicySet) -> Result<Self, GroupMutablePermissionsError> {
        let mut metadata_permissions_map: HashMap<String, MetadataPolicies> = HashMap::new();
        metadata_permissions_map.insert(
            MetadataField::GroupName.to_string(),
            policy_set.update_group_name_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::Description.to_string(),
            policy_set.update_group_description_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            policy_set.update_group_image_url_square_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::GroupPinnedFrameUrl.to_string(),
            policy_set.update_group_pinned_frame_url_policy.try_into()?,
        );

        Ok(PolicySet {
            add_member_policy: policy_set.add_member_policy.try_into()?,
            remove_member_policy: policy_set.remove_member_policy.try_into()?,
            add_admin_policy: policy_set.add_admin_policy.try_into()?,
            remove_admin_policy: policy_set.remove_admin_policy.try_into()?,
            update_metadata_policy: metadata_permissions_map,
            update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
        })
    }
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiMetadataField {
    GroupName,
    Description,
    ImageUrlSquare,
    PinnedFrameUrl,
}

impl From<&FfiMetadataField> for MetadataField {
    fn from(field: &FfiMetadataField) -> Self {
        match field {
            FfiMetadataField::GroupName => MetadataField::GroupName,
            FfiMetadataField::Description => MetadataField::Description,
            FfiMetadataField::ImageUrlSquare => MetadataField::GroupImageUrlSquare,
            FfiMetadataField::PinnedFrameUrl => MetadataField::GroupPinnedFrameUrl,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    pub async fn create_group(
        &self,
        account_addresses: Vec<String>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        log::info!(
            "creating group with account addresses: {}",
            account_addresses.join(", ")
        );

        if let Some(FfiGroupPermissionsOptions::CustomPolicy) = opts.permissions {
            if opts.custom_permission_policy_set.is_none() {
                return Err(GenericError::Generic {
                    err: "CustomPolicy must include policy set".to_string(),
                });
            }
        } else if opts.custom_permission_policy_set.is_some() {
            return Err(GenericError::Generic {
                err: "Only CustomPolicy may specify a policy set".to_string(),
            });
        }

        let metadata_options = opts.clone().into_group_metadata_options();

        let group_permissions = match opts.permissions {
            Some(FfiGroupPermissionsOptions::AllMembers) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AllMembers.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::CustomPolicy) => {
                if let Some(policy_set) = opts.custom_permission_policy_set {
                    Some(policy_set.try_into()?)
                } else {
                    None
                }
            }
            _ => None,
        };

        let convo = if account_addresses.is_empty() {
            self.inner_client
                .create_group(group_permissions, metadata_options)?
        } else {
            self.inner_client
                .create_group_with_members(account_addresses, group_permissions, metadata_options)
                .await?
        };

        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: convo.group_id,
            created_at_ns: convo.created_at_ns,
        });

        Ok(out)
    }

    pub async fn create_dm(&self, account_address: String) -> Result<Arc<FfiGroup>, GenericError> {
        log::info!("creating dm with target address: {}", account_address);

        let convo = self.inner_client.create_dm(account_address).await?;

        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: convo.group_id,
            created_at_ns: convo.created_at_ns,
        });

        Ok(out)
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        let inner = self.inner_client.as_ref();
        let group = inner
            .process_streamed_welcome_message(envelope_bytes)
            .await?;
        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: group.group_id,
            created_at_ns: group.created_at_ns,
        });
        Ok(out)
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        inner.sync_welcomes().await?;
        Ok(())
    }

    pub async fn sync_all_groups(&self) -> Result<u32, GenericError> {
        let inner = self.inner_client.as_ref();
        let groups = inner.find_groups(FindGroupParams {
            include_dm_groups: true,
            ..FindGroupParams::default()
        })?;

        log::info!(
            "groups for client inbox id {:?}: {:?}",
            self.inner_client.inbox_id(),
            groups.len()
        );

        let num_groups_synced: usize = inner.sync_all_groups(groups).await?;
        // Uniffi does not work with usize, so we need to convert to u32
        let num_groups_synced: u32 =
            num_groups_synced
                .try_into()
                .map_err(|_| GenericError::Generic {
                    err: "Failed to convert the number of synced groups from usize to u32"
                        .to_string(),
                })?;
        Ok(num_groups_synced)
    }

    pub async fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiGroup>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiGroup>> = inner
            .find_groups(FindGroupParams {
                allowed_states: None,
                created_after_ns: opts.created_after_ns,
                created_before_ns: opts.created_before_ns,
                limit: opts.limit,
                include_dm_groups: false,
            })?
            .into_iter()
            .map(|group| {
                Arc::new(FfiGroup {
                    inner_client: self.inner_client.clone(),
                    group_id: group.group_id,
                    created_at_ns: group.created_at_ns,
                })
            })
            .collect();

        Ok(convo_list)
    }

    pub async fn stream(&self, callback: Box<dyn FfiConversationCallback>) -> FfiStreamCloser {
        let client = self.inner_client.clone();
        let handle = RustXmtpClient::stream_conversations_with_callback(
            client.clone(),
            move |convo| {
                callback.on_conversation(Arc::new(FfiGroup {
                    inner_client: client.clone(),
                    group_id: convo.group_id,
                    created_at_ns: convo.created_at_ns,
                }))
            },
            false,
        );

        FfiStreamCloser::new(handle)
    }

    pub async fn stream_all_messages(
        &self,
        message_callback: Box<dyn FfiMessageCallback>,
    ) -> FfiStreamCloser {
        let handle = RustXmtpClient::stream_all_messages_with_callback(
            self.inner_client.clone(),
            move |message| message_callback.on_message(message.into()),
        );

        FfiStreamCloser::new(handle)
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroup {
    inner_client: Arc<RustXmtpClient>,
    group_id: Vec<u8>,
    created_at_ns: i64,
}

#[derive(uniffi::Record)]
pub struct FfiGroupMember {
    pub inbox_id: String,
    pub account_addresses: Vec<String>,
    pub installation_ids: Vec<Vec<u8>>,
    pub permission_level: FfiPermissionLevel,
    pub consent_state: FfiConsentState,
}

#[derive(uniffi::Enum)]
pub enum FfiPermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

#[derive(uniffi::Enum, PartialEq, Debug)]
pub enum FfiConsentState {
    Unknown,
    Allowed,
    Denied,
}

impl From<ConsentState> for FfiConsentState {
    fn from(state: ConsentState) -> Self {
        match state {
            ConsentState::Unknown => FfiConsentState::Unknown,
            ConsentState::Allowed => FfiConsentState::Allowed,
            ConsentState::Denied => FfiConsentState::Denied,
        }
    }
}

impl From<FfiConsentState> for ConsentState {
    fn from(state: FfiConsentState) -> Self {
        match state {
            FfiConsentState::Unknown => ConsentState::Unknown,
            FfiConsentState::Allowed => ConsentState::Allowed,
            FfiConsentState::Denied => ConsentState::Denied,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiConsentEntityType {
    GroupId,
    InboxId,
    Address,
}

impl From<FfiConsentEntityType> for ConsentType {
    fn from(entity_type: FfiConsentEntityType) -> Self {
        match entity_type {
            FfiConsentEntityType::GroupId => ConsentType::GroupId,
            FfiConsentEntityType::InboxId => ConsentType::InboxId,
            FfiConsentEntityType::Address => ConsentType::Address,
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiCreateGroupOptions {
    pub permissions: Option<FfiGroupPermissionsOptions>,
    pub group_name: Option<String>,
    pub group_image_url_square: Option<String>,
    pub group_description: Option<String>,
    pub group_pinned_frame_url: Option<String>,
    pub custom_permission_policy_set: Option<FfiPermissionPolicySet>,
}

impl FfiCreateGroupOptions {
    pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
        GroupMetadataOptions {
            name: self.group_name,
            image_url_square: self.group_image_url_square,
            description: self.group_description,
            pinned_frame_url: self.group_pinned_frame_url,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiGroup {
    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let message_id = group
            .send_message(content_bytes.as_slice(), &self.inner_client)
            .await?;
        Ok(message_id)
    }

    /// send a message without immediately publishing to the delivery service.
    pub fn send_optimistic(&self, content_bytes: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let id = group.send_message_optimistic(content_bytes.as_slice())?;

        Ok(id)
    }

    /// Publish all unpublished messages
    pub async fn publish_messages(&self) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.publish_messages(&self.inner_client).await?;
        Ok(())
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.sync(&self.inner_client).await?;

        Ok(())
    }

    pub fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let delivery_status = opts.delivery_status.map(|status| status.into());

        let messages: Vec<FfiMessage> = group
            .find_messages(
                None,
                opts.sent_before_ns,
                opts.sent_after_ns,
                delivery_status,
                opts.limit,
            )?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }

    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<FfiMessage, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        let message = group
            .process_streamed_group_message(envelope_bytes, &self.inner_client)
            .await?;
        let ffi_message = message.into();

        Ok(ffi_message)
    }

    pub async fn list_members(&self) -> Result<Vec<FfiGroupMember>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let members: Vec<FfiGroupMember> = group
            .members(&self.inner_client)
            .await?
            .into_iter()
            .map(|member| FfiGroupMember {
                inbox_id: member.inbox_id,
                account_addresses: member.account_addresses,
                installation_ids: member.installation_ids,
                permission_level: match member.permission_level {
                    PermissionLevel::Member => FfiPermissionLevel::Member,
                    PermissionLevel::Admin => FfiPermissionLevel::Admin,
                    PermissionLevel::SuperAdmin => FfiPermissionLevel::SuperAdmin,
                },
                consent_state: member.consent_state.into(),
            })
            .collect();

        Ok(members)
    }

    pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        log::info!("adding members: {}", account_addresses.join(","));

        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .add_members(&self.inner_client, account_addresses)
            .await?;

        Ok(())
    }

    pub async fn add_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        log::info!("adding members by inbox id: {}", inbox_ids.join(","));

        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .add_members_by_inbox_id(&self.inner_client, inbox_ids)
            .await?;

        Ok(())
    }

    pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .remove_members(&self.inner_client, account_addresses)
            .await?;

        Ok(())
    }

    pub async fn remove_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .remove_members_by_inbox_id(&self.inner_client, inbox_ids)
            .await?;

        Ok(())
    }

    pub async fn update_group_name(&self, group_name: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .update_group_name(&self.inner_client, group_name)
            .await?;

        Ok(())
    }

    pub fn group_name(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let provider = group.mls_provider()?;
        let group_name = group.group_name(&provider)?;

        Ok(group_name)
    }

    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .update_group_image_url_square(&self.inner_client, group_image_url_square)
            .await?;

        Ok(())
    }

    pub fn group_image_url_square(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let group_image_url_square = group.group_image_url_square(group.mls_provider()?)?;

        Ok(group_image_url_square)
    }

    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .update_group_description(&self.inner_client, group_description)
            .await?;

        Ok(())
    }

    pub fn group_description(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let group_description = group.group_description(group.mls_provider()?)?;

        Ok(group_description)
    }

    pub async fn update_group_pinned_frame_url(
        &self,
        pinned_frame_url: String,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .update_group_pinned_frame_url(&self.inner_client, pinned_frame_url)
            .await?;

        Ok(())
    }

    pub fn group_pinned_frame_url(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let group_pinned_frame_url = group.group_pinned_frame_url(group.mls_provider()?)?;

        Ok(group_pinned_frame_url)
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let admin_list = group.admin_list(group.mls_provider()?)?;

        Ok(admin_list)
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let super_admin_list = group.super_admin_list(group.mls_provider()?)?;

        Ok(super_admin_list)
    }

    pub fn is_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        let admin_list = self.admin_list()?;
        Ok(admin_list.contains(inbox_id))
    }

    pub fn is_super_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        let super_admin_list = self.super_admin_list()?;
        Ok(super_admin_list.contains(inbox_id))
    }

    pub async fn add_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group
            .update_admin_list(&self.inner_client, UpdateAdminListType::Add, inbox_id)
            .await?;

        Ok(())
    }

    pub async fn remove_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group
            .update_admin_list(&self.inner_client, UpdateAdminListType::Remove, inbox_id)
            .await?;

        Ok(())
    }

    pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group
            .update_admin_list(&self.inner_client, UpdateAdminListType::AddSuper, inbox_id)
            .await?;

        Ok(())
    }

    pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group
            .update_admin_list(
                &self.inner_client,
                UpdateAdminListType::RemoveSuper,
                inbox_id,
            )
            .await?;

        Ok(())
    }

    pub fn group_permissions(&self) -> Result<Arc<FfiGroupPermissions>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let permissions = group.permissions()?;
        Ok(Arc::new(FfiGroupPermissions {
            inner: Arc::new(permissions),
        }))
    }

    pub async fn update_permission_policy(
        &self,
        permission_update_type: FfiPermissionUpdateType,
        permission_policy_option: FfiPermissionPolicy,
        metadata_field: Option<FfiMetadataField>,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group
            .update_permission_policy(
                &self.inner_client,
                PermissionUpdateType::from(&permission_update_type),
                permission_policy_option.try_into()?,
                metadata_field.map(|field| MetadataField::from(&field)),
            )
            .await
            .map_err(|e| GenericError::from(e.to_string()))?;
        Ok(())
    }

    pub async fn stream(&self, message_callback: Box<dyn FfiMessageCallback>) -> FfiStreamCloser {
        let inner_client = Arc::clone(&self.inner_client);
        let handle = MlsGroup::stream_with_callback(
            inner_client,
            self.group_id.clone(),
            self.created_at_ns,
            move |message| message_callback.on_message(message.into()),
        );

        FfiStreamCloser::new(handle)
    }

    pub fn created_at_ns(&self) -> i64 {
        self.created_at_ns
    }

    pub fn is_active(&self) -> Result<bool, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.is_active(group.mls_provider()?)?)
    }

    pub fn consent_state(&self) -> Result<FfiConsentState, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let state = group.consent_state()?;

        Ok(state.into())
    }

    pub fn update_consent_state(&self, state: FfiConsentState) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.update_consent_state(state.into())?;

        Ok(())
    }

    pub fn added_by_inbox_id(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.added_by_inbox_id()?)
    }

    pub fn group_metadata(&self) -> Result<Arc<FfiGroupMetadata>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let metadata = group.metadata(group.mls_provider()?)?;
        Ok(Arc::new(FfiGroupMetadata {
            inner: Arc::new(metadata),
        }))
    }
}

#[uniffi::export]
impl FfiGroup {
    pub fn id(&self) -> Vec<u8> {
        self.group_id.clone()
    }
}

#[derive(uniffi::Enum, PartialEq)]
pub enum FfiGroupMessageKind {
    Application,
    MembershipChange,
}

impl From<GroupMessageKind> for FfiGroupMessageKind {
    fn from(kind: GroupMessageKind) -> Self {
        match kind {
            GroupMessageKind::Application => FfiGroupMessageKind::Application,
            GroupMessageKind::MembershipChange => FfiGroupMessageKind::MembershipChange,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiDeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

impl From<DeliveryStatus> for FfiDeliveryStatus {
    fn from(status: DeliveryStatus) -> Self {
        match status {
            DeliveryStatus::Unpublished => FfiDeliveryStatus::Unpublished,
            DeliveryStatus::Published => FfiDeliveryStatus::Published,
            DeliveryStatus::Failed => FfiDeliveryStatus::Failed,
        }
    }
}

impl From<FfiDeliveryStatus> for DeliveryStatus {
    fn from(status: FfiDeliveryStatus) -> Self {
        match status {
            FfiDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
            FfiDeliveryStatus::Published => DeliveryStatus::Published,
            FfiDeliveryStatus::Failed => DeliveryStatus::Failed,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub convo_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content: Vec<u8>,
    pub kind: FfiGroupMessageKind,
    pub delivery_status: FfiDeliveryStatus,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            convo_id: msg.group_id,
            sender_inbox_id: msg.sender_inbox_id,
            content: msg.decrypted_message_bytes,
            kind: msg.kind.into(),
            delivery_status: msg.delivery_status.into(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiConsent {
    pub entity_type: FfiConsentEntityType,
    pub state: FfiConsentState,
    pub entity: String,
}

impl From<FfiConsent> for StoredConsentRecord {
    fn from(consent: FfiConsent) -> Self {
        Self {
            entity_type: consent.entity_type.into(),
            state: consent.state.into(),
            entity: consent.entity,
        }
    }
}

#[derive(uniffi::Object, Clone, Debug)]
pub struct FfiStreamCloser {
    #[allow(clippy::type_complexity)]
    stream_handle: Arc<Mutex<Option<StreamHandle<Result<(), ClientError>>>>>,
    // for convenience, does not require locking mutex.
    abort_handle: Arc<AbortHandle>,
}

impl FfiStreamCloser {
    pub fn new(stream_handle: StreamHandle<Result<(), ClientError>>) -> Self {
        Self {
            abort_handle: Arc::new(stream_handle.handle.abort_handle()),
            stream_handle: Arc::new(Mutex::new(Some(stream_handle))),
        }
    }

    #[cfg(test)]
    pub async fn wait_for_ready(&self) {
        let mut handle = self.stream_handle.lock().await;
        if let Some(ref mut h) = &mut *handle {
            h.wait_for_ready().await;
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiStreamCloser {
    /// Signal the stream to end
    /// Does not wait for the stream to end.
    pub fn end(&self) {
        self.abort_handle.abort();
    }

    /// End the stream and asyncronously wait for it to shutdown
    pub async fn end_and_wait(&self) -> Result<(), GenericError> {
        if self.abort_handle.is_finished() {
            return Ok(());
        }

        let mut stream_handle = self.stream_handle.lock().await;
        let stream_handle = stream_handle.take();
        if let Some(h) = stream_handle {
            h.handle.abort();
            match h.handle.await {
                Err(e) if !e.is_cancelled() => Err(GenericError::Generic {
                    err: format!("subscription event loop join error {}", e),
                }),
                Err(e) if e.is_cancelled() => Ok(()),
                Ok(t) => t.map_err(|e| GenericError::Generic { err: e.to_string() }),
                Err(e) => Err(GenericError::Generic {
                    err: format!("error joining task {}", e),
                }),
            }
        } else {
            log::warn!("subscription already closed");
            Ok(())
        }
    }

    pub fn is_closed(&self) -> bool {
        self.abort_handle.is_finished()
    }
}

#[uniffi::export(callback_interface)]
pub trait FfiMessageCallback: Send + Sync {
    fn on_message(&self, message: FfiMessage);
}

#[uniffi::export(callback_interface)]
pub trait FfiConversationCallback: Send + Sync {
    fn on_conversation(&self, conversation: Arc<FfiGroup>);
}

#[derive(uniffi::Object)]
pub struct FfiGroupMetadata {
    inner: Arc<GroupMetadata>,
}

#[uniffi::export]
impl FfiGroupMetadata {
    pub fn creator_inbox_id(&self) -> String {
        self.inner.creator_inbox_id.clone()
    }

    pub fn conversation_type(&self) -> String {
        match self.inner.conversation_type {
            ConversationType::Group => "group".to_string(),
            ConversationType::Dm => "dm".to_string(),
            ConversationType::Sync => "sync".to_string(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroupPermissions {
    inner: Arc<GroupMutablePermissions>,
}

#[uniffi::export]
impl FfiGroupPermissions {
    pub fn policy_type(&self) -> Result<FfiGroupPermissionsOptions, GenericError> {
        if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
            Ok(preconfigured_policy.into())
        } else {
            Ok(FfiGroupPermissionsOptions::CustomPolicy)
        }
    }

    pub fn policy_set(&self) -> Result<FfiPermissionPolicySet, GenericError> {
        let policy_set = &self.inner.policies;
        let metadata_policy_map = &policy_set.update_metadata_policy;
        let get_policy = |field: &str| {
            metadata_policy_map
                .get(field)
                .map(FfiPermissionPolicy::from)
                .unwrap_or(FfiPermissionPolicy::DoesNotExist)
        };
        Ok(FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::from(&policy_set.add_member_policy),
            remove_member_policy: FfiPermissionPolicy::from(&policy_set.remove_member_policy),
            add_admin_policy: FfiPermissionPolicy::from(&policy_set.add_admin_policy),
            remove_admin_policy: FfiPermissionPolicy::from(&policy_set.remove_admin_policy),
            update_group_name_policy: get_policy(MetadataField::GroupName.as_str()),
            update_group_description_policy: get_policy(MetadataField::Description.as_str()),
            update_group_image_url_square_policy: get_policy(
                MetadataField::GroupImageUrlSquare.as_str(),
            ),
            update_group_pinned_frame_url_policy: get_policy(
                MetadataField::GroupPinnedFrameUrl.as_str(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{create_client, FfiMessage, FfiMessageCallback, FfiXmtpClient};
    use crate::{
        get_inbox_id_for_address, inbox_owner::SigningError, logger::FfiLogger, FfiConsent,
        FfiConsentEntityType, FfiConsentState, FfiConversationCallback, FfiCreateGroupOptions,
        FfiGroup, FfiGroupMessageKind, FfiGroupPermissionsOptions, FfiInboxOwner,
        FfiListConversationsOptions, FfiListMessagesOptions, FfiMetadataField, FfiPermissionPolicy,
        FfiPermissionPolicySet, FfiPermissionUpdateType,
    };
    use ethers::utils::hex;
    use rand::distributions::{Alphanumeric, DistString};
    use std::{
        env,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc, Mutex,
        },
    };
    use tokio::{sync::Notify, time::error::Elapsed};
    use xmtp_cryptography::{signature::RecoverableSignature, utils::rng};
    use xmtp_id::associations::{
        generate_inbox_id,
        unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
    };
    use xmtp_mls::{
        groups::{GroupError, MlsGroup},
        storage::EncryptionKey,
        InboxOwner,
    };

    #[derive(Clone)]
    pub struct LocalWalletInboxOwner {
        wallet: xmtp_cryptography::utils::LocalWallet,
    }

    impl LocalWalletInboxOwner {
        pub fn with_wallet(wallet: xmtp_cryptography::utils::LocalWallet) -> Self {
            Self { wallet }
        }

        pub fn new() -> Self {
            Self {
                wallet: xmtp_cryptography::utils::LocalWallet::new(&mut rng()),
            }
        }
    }

    impl FfiInboxOwner for LocalWalletInboxOwner {
        fn get_address(&self) -> String {
            self.wallet.get_address()
        }

        fn sign(&self, text: String) -> Result<Vec<u8>, SigningError> {
            let recoverable_signature =
                self.wallet.sign(&text).map_err(|_| SigningError::Generic)?;
            match recoverable_signature {
                RecoverableSignature::Eip191Signature(signature_bytes) => Ok(signature_bytes),
            }
        }
    }

    pub struct MockLogger {}

    impl FfiLogger for MockLogger {
        fn log(&self, _level: u32, level_label: String, message: String) {
            println!("[{}]{}", level_label, message)
        }
    }

    #[derive(Default, Clone)]
    struct RustStreamCallback {
        num_messages: Arc<AtomicU32>,
        messages: Arc<Mutex<Vec<FfiMessage>>>,
        conversations: Arc<Mutex<Vec<Arc<FfiGroup>>>>,
        notify: Arc<Notify>,
    }

    impl RustStreamCallback {
        pub fn message_count(&self) -> u32 {
            self.num_messages.load(Ordering::SeqCst)
        }

        pub async fn wait_for_delivery(&self) -> Result<(), Elapsed> {
            tokio::time::timeout(std::time::Duration::from_secs(60), async {
                self.notify.notified().await
            })
            .await?;
            Ok(())
        }
    }

    impl FfiMessageCallback for RustStreamCallback {
        fn on_message(&self, message: FfiMessage) {
            let mut messages = self.messages.lock().unwrap();
            log::info!(
                "ON MESSAGE Received\n-------- \n{}\n----------",
                String::from_utf8_lossy(&message.content)
            );
            messages.push(message);
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
            self.notify.notify_one();
        }
    }

    impl FfiConversationCallback for RustStreamCallback {
        fn on_conversation(&self, group: Arc<super::FfiGroup>) {
            log::debug!("received conversation");
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
            let mut convos = self.conversations.lock().unwrap();
            convos.push(group);
            self.notify.notify_one();
        }
    }

    pub fn rand_string() -> String {
        Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
    }

    pub fn tmp_path() -> String {
        let db_name = rand_string();
        format!("{}/{}.db3", env::temp_dir().to_str().unwrap(), db_name)
    }

    fn static_enc_key() -> EncryptionKey {
        [2u8; 32]
    }

    async fn register_client(inbox_owner: &LocalWalletInboxOwner, client: &FfiXmtpClient) {
        let signature_request = client.signature_request().unwrap();
        signature_request
            .add_ecdsa_signature(
                inbox_owner
                    .sign(signature_request.signature_text().await.unwrap())
                    .unwrap(),
            )
            .await
            .unwrap();
        client.register_identity(signature_request).await.unwrap();
    }

    /// Create a new test client with a given wallet.
    async fn new_test_client_with_wallet(
        wallet: xmtp_cryptography::utils::LocalWallet,
    ) -> Arc<FfiXmtpClient> {
        let ffi_inbox_owner = LocalWalletInboxOwner::with_wallet(wallet);
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            Some(xmtp_mls::storage::EncryptedMessageStore::generate_enc_key().into()),
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();

        register_client(&ffi_inbox_owner, &client).await;
        client
    }

    async fn new_test_client() -> Arc<FfiXmtpClient> {
        let wallet = xmtp_cryptography::utils::LocalWallet::new(&mut rng());
        new_test_client_with_wallet(wallet).await
    }

    impl FfiGroup {
        #[cfg(test)]
        async fn update_installations(&self) -> Result<(), GroupError> {
            let group = MlsGroup::new(
                self.inner_client.context().clone(),
                self.group_id.clone(),
                self.created_at_ns,
            );

            group.update_installations(&self.inner_client).await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn get_inbox_id() {
        let client = new_test_client().await;
        let real_inbox_id = client.inbox_id();

        let from_network = get_inbox_id_for_address(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            client.account_address.clone(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(real_inbox_id, from_network);
    }

    #[tokio::test]
    #[ignore]
    async fn test_legacy_identity() {
        let account_address = "0x0bD00B21aF9a2D538103c3AAf95Cb507f8AF1B28".to_lowercase();
        let legacy_keys = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();
        let nonce = 0;
        let inbox_id = generate_inbox_id(&account_address, &nonce);

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
            &inbox_id,
            account_address.to_string(),
            nonce,
            Some(legacy_keys),
            None,
        )
        .await
        .unwrap();

        assert!(client.signature_request().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_storage() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let path = tmp_path();

        let client_a = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();
        register_client(&ffi_inbox_owner, &client_a).await;

        let installation_pub_key = client_a.inner_client.installation_public_key();
        drop(client_a);

        let client_b = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
            None,
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();

        let other_installation_pub_key = client_b.inner_client.installation_public_key();
        drop(client_b);

        assert!(
            installation_pub_key == other_installation_pub_key,
            "did not use same installation ID"
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_key() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let path = tmp_path();

        let key = static_enc_key().to_vec();

        let client_a = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();

        drop(client_a);

        let mut other_key = static_enc_key();
        other_key[31] = 1;

        let result_errored = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
            Some(other_key.to_vec()),
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .is_err();

        assert!(result_errored, "did not error on wrong encryption key")
    }

    use super::FfiSignatureRequest;
    async fn sign_with_wallet(
        wallet: &xmtp_cryptography::utils::LocalWallet,
        signature_request: &FfiSignatureRequest,
    ) {
        let scw_verifier = signature_request.scw_verifier.clone();
        let signature_text = signature_request.inner.lock().await.signature_text();
        let wallet_signature: Vec<u8> = wallet.sign(&signature_text.clone()).unwrap().into();

        signature_request
            .inner
            .lock()
            .await
            .add_signature(
                UnverifiedSignature::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature::new(
                    wallet_signature,
                )),
                scw_verifier.clone().as_ref(),
            )
            .await
            .unwrap();
    }

    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_add_wallet_to_inbox() {
        // Setup the initial first client
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let path = tmp_path();
        let key = static_enc_key().to_vec();
        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client(&ffi_inbox_owner, &client).await;

        sign_with_wallet(&ffi_inbox_owner.wallet, &signature_request).await;

        let conn = client.inner_client.store().conn().unwrap();
        let state = client
            .inner_client
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(state.members().len(), 2);

        // Now, add the second wallet to the client

        let wallet_to_add = generate_local_wallet();
        let new_account_address = wallet_to_add.get_address();
        println!("second address: {}", new_account_address);

        let signature_request = client
            .add_wallet(&ffi_inbox_owner.get_address(), &new_account_address)
            .await
            .expect("could not add wallet");

        sign_with_wallet(&ffi_inbox_owner.wallet, &signature_request).await;
        sign_with_wallet(&wallet_to_add, &signature_request).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let updated_state = client
            .inner_client
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(updated_state.members().len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_revoke_wallet() {
        // Setup the initial first client
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let path = tmp_path();
        let key = static_enc_key().to_vec();
        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client(&ffi_inbox_owner, &client).await;

        sign_with_wallet(&ffi_inbox_owner.wallet, &signature_request).await;

        let conn = client.inner_client.store().conn().unwrap();
        let state = client
            .inner_client
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(state.members().len(), 2);

        // Now, add the second wallet to the client

        let wallet_to_add = generate_local_wallet();
        let new_account_address = wallet_to_add.get_address();
        println!("second address: {}", new_account_address);

        let signature_request = client
            .add_wallet(&ffi_inbox_owner.get_address(), &new_account_address)
            .await
            .expect("could not add wallet");

        sign_with_wallet(&ffi_inbox_owner.wallet, &signature_request).await;
        sign_with_wallet(&wallet_to_add, &signature_request).await;

        client
            .apply_signature_request(signature_request.clone())
            .await
            .unwrap();

        let updated_state = client
            .inner_client
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(updated_state.members().len(), 3);

        // Now, revoke the second wallet
        let signature_request = client
            .revoke_wallet(&new_account_address)
            .await
            .expect("could not revoke wallet");

        sign_with_wallet(&ffi_inbox_owner.wallet, &signature_request).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let revoked_state = client
            .inner_client
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(revoked_state.members().len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_invalid_external_signature() {
        let inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&inbox_owner.get_address(), &nonce);
        let path = tmp_path();

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None, // encryption_key
            &inbox_id,
            inbox_owner.get_address(),
            nonce,
            None, // v2_signed_private_key_proto
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap();
        assert!(client.register_identity(signature_request).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_message() {
        let amal = LocalWalletInboxOwner::new();
        let nonce = 1;
        let amal_inbox_id = generate_inbox_id(&amal.get_address(), &nonce);
        let bola = LocalWalletInboxOwner::new();
        let bola_inbox_id = generate_inbox_id(&bola.get_address(), &nonce);
        let path = tmp_path();

        let client_amal = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            &amal_inbox_id,
            amal.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();
        let can_message_result = client_amal
            .can_message(vec![bola.get_address()])
            .await
            .unwrap();

        assert!(
            can_message_result
                .get(&bola.get_address().to_string())
                .map(|&value| !value)
                .unwrap_or(false),
            "Expected the can_message result to be false for the address"
        );

        let client_bola = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            &bola_inbox_id,
            bola.get_address(),
            nonce,
            None,
            None,
        )
        .await
        .unwrap();
        register_client(&bola, &client_bola).await;

        let can_message_result2 = client_amal
            .can_message(vec![bola.get_address()])
            .await
            .unwrap();

        assert!(
            can_message_result2
                .get(&bola.get_address().to_string())
                .copied()
                .unwrap_or(false),
            "Expected the can_message result to be true for the address"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_group_with_members() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let members = group.list_members().await.unwrap();
        assert_eq!(members.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_group_with_metadata() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions {
                    permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
                    group_name: Some("Group Name".to_string()),
                    group_image_url_square: Some("url".to_string()),
                    group_description: Some("group description".to_string()),
                    group_pinned_frame_url: Some("pinned frame".to_string()),
                    custom_permission_policy_set: None,
                },
            )
            .await
            .unwrap();

        let members = group.list_members().await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(group.group_name().unwrap(), "Group Name");
        assert_eq!(group.group_image_url_square().unwrap(), "url");
        assert_eq!(group.group_description().unwrap(), "group description");
        assert_eq!(group.group_pinned_frame_url().unwrap(), "pinned frame");
    }

    // Looks like this test might be a separate issue
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    #[ignore]
    async fn test_can_stream_group_messages_for_updates() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callbacks = RustStreamCallback::default();
        let stream_messages = bo
            .conversations()
            .stream_all_messages(Box::new(message_callbacks.clone()))
            .await;
        stream_messages.wait_for_ready().await;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        alix_group
            .update_group_name("Old Name".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();

        let bo_groups = bo
            .conversations()
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap();
        let bo_group = &bo_groups[0];
        bo_group.sync().await.unwrap();
        bo_group
            .update_group_name("Old Name2".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();

        // Uncomment the following lines to add more group name updates
        bo_group
            .update_group_name("Old Name3".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();

        assert_eq!(message_callbacks.message_count(), 3);

        stream_messages.end_and_wait().await.unwrap();

        assert!(stream_messages.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_sync_all_groups() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        for _i in 0..30 {
            alix.conversations()
                .create_group(
                    vec![bo.account_address.clone()],
                    FfiCreateGroupOptions::default(),
                )
                .await
                .unwrap();
        }

        bo.conversations().sync().await.unwrap();
        let alix_groups = alix
            .conversations()
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap();

        let alix_group1 = alix_groups[0].clone();
        let alix_group5 = alix_groups[5].clone();
        let bo_group1 = bo.group(alix_group1.id()).unwrap();
        let bo_group5 = bo.group(alix_group5.id()).unwrap();

        alix_group1.send("alix1".as_bytes().to_vec()).await.unwrap();
        alix_group5.send("alix1".as_bytes().to_vec()).await.unwrap();

        let bo_messages1 = bo_group1
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let bo_messages5 = bo_group5
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages1.len(), 0);
        assert_eq!(bo_messages5.len(), 0);

        bo.conversations().sync_all_groups().await.unwrap();

        let bo_messages1 = bo_group1
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let bo_messages5 = bo_group5
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages1.len(), 1);
        assert_eq!(bo_messages5.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_sync_all_groups_active_only() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Create 30 groups with alix and bo and sync them
        for _i in 0..30 {
            alix.conversations()
                .create_group(
                    vec![bo.account_address.clone()],
                    FfiCreateGroupOptions::default(),
                )
                .await
                .unwrap();
        }
        bo.conversations().sync().await.unwrap();
        let num_groups_synced_1: u32 = bo.conversations().sync_all_groups().await.unwrap();
        assert!(num_groups_synced_1 == 30);

        // Remove bo from all groups and sync
        for group in alix
            .conversations()
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap()
        {
            group
                .remove_members(vec![bo.account_address.clone()])
                .await
                .unwrap();
        }

        // First sync after removal needs to process all groups and set them to inactive
        let num_groups_synced_2: u32 = bo.conversations().sync_all_groups().await.unwrap();
        assert!(num_groups_synced_2 == 30);

        // Second sync after removal will not process inactive groups
        let num_groups_synced_3: u32 = bo.conversations().sync_all_groups().await.unwrap();
        assert!(num_groups_synced_3 == 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_send_message_when_out_of_sync() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;
        let davon = new_test_client().await;
        let eri = new_test_client().await;
        let frankie = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.group(alix_group.id()).unwrap();

        bo_group.send("bo1".as_bytes().to_vec()).await.unwrap();
        alix_group.send("alix1".as_bytes().to_vec()).await.unwrap();

        // Move the group forward by 3 epochs (as Alix's max_past_epochs is
        // configured to 3) without Bo syncing
        alix_group
            .add_members(vec![
                caro.account_address.clone(),
                davon.account_address.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .remove_members(vec![
                caro.account_address.clone(),
                davon.account_address.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .add_members(vec![
                eri.account_address.clone(),
                frankie.account_address.clone(),
            ])
            .await
            .unwrap();

        // Bo sends messages to Alix while 3 epochs behind
        bo_group.send("bo3".as_bytes().to_vec()).await.unwrap();
        alix_group.send("alix3".as_bytes().to_vec()).await.unwrap();
        bo_group.send("bo4".as_bytes().to_vec()).await.unwrap();
        bo_group.send("bo5".as_bytes().to_vec()).await.unwrap();

        alix_group.sync().await.unwrap();
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();

        bo_group.sync().await.unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages.len(), 9);
        assert_eq!(alix_messages.len(), 10);

        assert_eq!(
            bo_messages[bo_messages.len() - 1].id,
            alix_messages[alix_messages.len() - 1].id
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_create_new_installation_without_breaking_group() {
        let wallet1_key = &mut rng();
        let wallet1 = xmtp_cryptography::utils::LocalWallet::new(wallet1_key);
        let wallet2_key = &mut rng();
        let wallet2 = xmtp_cryptography::utils::LocalWallet::new(wallet2_key);

        // Create clients
        let client1 = new_test_client_with_wallet(wallet1).await;
        let client2 = new_test_client_with_wallet(wallet2.clone()).await;
        // Create a new group with client1 including wallet2

        let group = client1
            .conversations()
            .create_group(
                vec![client2.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Sync groups
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // Find groups for both clients
        let client1_group = client1.group(group.id()).unwrap();
        let client2_group = client2.group(group.id()).unwrap();

        // Sync both groups
        client1_group.sync().await.unwrap();
        client2_group.sync().await.unwrap();

        // Assert both clients see 2 members
        let client1_members = client1_group.list_members().await.unwrap();
        assert_eq!(client1_members.len(), 2);

        let client2_members = client2_group.list_members().await.unwrap();
        assert_eq!(client2_members.len(), 2);

        // Drop and delete local database for client2
        client2.release_db_connection().unwrap();

        // Recreate client2 (new installation)
        let client2 = new_test_client_with_wallet(wallet2).await;

        client1_group.update_installations().await.unwrap();

        // Send a message that will break the group
        client1_group
            .send("This message will break the group".as_bytes().to_vec())
            .await
            .unwrap();

        // Assert client1 still sees 2 members
        let client1_members = client1_group.list_members().await.unwrap();
        assert_eq!(client1_members.len(), 2);

        client2.conversations().sync().await.unwrap();
        let client2_group = client2.group(group.id()).unwrap();
        let client2_members = client2_group.list_members().await.unwrap();
        assert_eq!(client2_members.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_create_new_installations_does_not_fork_group() {
        let bo_wallet_key = &mut rng();
        let bo_wallet = xmtp_cryptography::utils::LocalWallet::new(bo_wallet_key);

        // Create clients
        let alix = new_test_client().await;
        let bo = new_test_client_with_wallet(bo_wallet.clone()).await;
        let caro = new_test_client().await;

        // Alix begins a stream for all messages
        let message_callbacks = RustStreamCallback::default();
        let stream_messages = alix
            .conversations()
            .stream_all_messages(Box::new(message_callbacks.clone()))
            .await;
        stream_messages.wait_for_ready().await;

        // Alix creates a group with Bo and Caro
        let group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone(), caro.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Alix and Caro Sync groups
        alix.conversations().sync().await.unwrap();
        bo.conversations().sync().await.unwrap();
        caro.conversations().sync().await.unwrap();

        // Alix and Caro find the group
        let alix_group = alix.group(group.id()).unwrap();
        let bo_group = bo.group(group.id()).unwrap();
        let caro_group = caro.group(group.id()).unwrap();

        alix_group.update_installations().await.unwrap();
        log::info!("Alix sending first message");
        // Alix sends a message in the group
        alix_group
            .send("First message".as_bytes().to_vec())
            .await
            .unwrap();

        log::info!("Caro sending second message");
        caro_group.update_installations().await.unwrap();
        // Caro sends a message in the group
        caro_group
            .send("Second message".as_bytes().to_vec())
            .await
            .unwrap();

        // Bo logs back in with a new installation
        let bo2 = new_test_client_with_wallet(bo_wallet).await;

        // Bo begins a stream for all messages
        let bo_message_callbacks = RustStreamCallback::default();
        let bo_stream_messages = bo2
            .conversations()
            .stream_all_messages(Box::new(bo_message_callbacks.clone()))
            .await;
        bo_stream_messages.wait_for_ready().await;

        alix_group.update_installations().await.unwrap();

        log::info!("Alix sending third message after Bo's second installation added");
        // Alix sends a message to the group
        alix_group
            .send("Third message".as_bytes().to_vec())
            .await
            .unwrap();

        // New installation of bo finds the group
        bo2.conversations().sync().await.unwrap();
        let bo2_group = bo2.group(group.id()).unwrap();

        log::info!("Bo sending fourth message");
        // Bo sends a message to the group
        bo2_group.update_installations().await.unwrap();
        bo2_group
            .send("Fourth message".as_bytes().to_vec())
            .await
            .unwrap();

        log::info!("Caro sending fifth message");
        // Caro sends a message in the group
        caro_group.update_installations().await.unwrap();
        caro_group
            .send("Fifth message".as_bytes().to_vec())
            .await
            .unwrap();

        log::info!("Syncing alix");
        alix_group.sync().await.unwrap();
        log::info!("Syncing bo 1");
        bo_group.sync().await.unwrap();
        log::info!("Syncing bo 2");
        bo2_group.sync().await.unwrap();
        log::info!("Syncing caro");
        caro_group.sync().await.unwrap();

        // Get the message count for all the clients
        let caro_messages = caro_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let bo2_messages = bo2_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();

        assert_eq!(caro_messages.len(), 5);
        assert_eq!(alix_messages.len(), 6);
        assert_eq!(bo_messages.len(), 5);
        // Bo 2 only sees three messages since it joined after the first 2 were sent
        assert_eq!(bo2_messages.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_send_messages_when_epochs_behind() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();

        let bo_group = bo.group(alix_group.id()).unwrap();

        // Move forward 4 epochs
        alix_group
            .update_group_description("change 1".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 2".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 3".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 4".to_string())
            .await
            .unwrap();

        bo_group
            .send("bo message 1".as_bytes().to_vec())
            .await
            .unwrap();

        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();

        let alix_can_see_bo_message = alix_messages
            .iter()
            .any(|message| message.content == "bo message 1".as_bytes());
        assert!(
            alix_can_see_bo_message,
            "\"bo message 1\" not found in alix's messages"
        );

        let bo_can_see_bo_message = bo_messages
            .iter()
            .any(|message| message.content == "bo message 1".as_bytes());
        assert!(
            bo_can_see_bo_message,
            "\"bo message 1\" not found in bo's messages"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_add_members_when_out_of_sync() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;
        let davon = new_test_client().await;
        let eri = new_test_client().await;
        let frankie = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.group(alix_group.id()).unwrap();

        bo_group.send("bo1".as_bytes().to_vec()).await.unwrap();
        alix_group.send("alix1".as_bytes().to_vec()).await.unwrap();

        // Move the group forward by 3 epochs (as Alix's max_past_epochs is
        // configured to 3) without Bo syncing
        alix_group
            .add_members(vec![
                caro.account_address.clone(),
                davon.account_address.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .remove_members(vec![
                caro.account_address.clone(),
                davon.account_address.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .add_members(vec![eri.account_address.clone()])
            .await
            .unwrap();

        // Bo adds a member while 3 epochs behind
        bo_group
            .add_members(vec![frankie.account_address.clone()])
            .await
            .unwrap();

        bo_group.sync().await.unwrap();
        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 4);

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_removed_members_no_longer_update() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.group(alix_group.id()).unwrap();

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 2);

        bo_group.sync().await.unwrap();
        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 2);

        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages.len(), 0);

        alix_group
            .remove_members(vec![bo.account_address.clone()])
            .await
            .unwrap();

        alix_group.send("hello".as_bytes().to_vec()).await.unwrap();

        bo_group.sync().await.unwrap();
        assert!(!bo_group.is_active().unwrap());

        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert!(bo_messages.first().unwrap().kind == FfiGroupMessageKind::MembershipChange);
        assert_eq!(bo_messages.len(), 1);

        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 1);

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 1);
    }

    // test is also showing intermittent failures with database locked msg
    #[ignore]
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_and_update_name_without_forking_group() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callbacks = RustStreamCallback::default();
        let stream_messages = bo
            .conversations()
            .stream_all_messages(Box::new(message_callbacks.clone()))
            .await;
        stream_messages.wait_for_ready().await;

        let first_msg_check = 2;
        let second_msg_check = 5;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        alix_group
            .update_group_name("hello".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();
        alix_group.send("hello1".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();

        let bo_groups = bo
            .conversations()
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_groups.len(), 1);
        let bo_group = bo_groups[0].clone();
        bo_group.sync().await.unwrap();

        let bo_messages1 = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages1.len(), first_msg_check);

        bo_group.send("hello2".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();
        bo_group.send("hello3".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();

        alix_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(alix_messages.len(), second_msg_check);

        alix_group.send("hello4".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await.unwrap();
        bo_group.sync().await.unwrap();

        let bo_messages2 = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(bo_messages2.len(), second_msg_check);
        assert_eq!(message_callbacks.message_count(), second_msg_check as u32);

        stream_messages.end_and_wait().await.unwrap();
        assert!(stream_messages.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_conversation_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let stream_callback = RustStreamCallback::default();

        let stream = bola
            .conversations()
            .stream(Box::new(stream_callback.clone()))
            .await;

        amal.conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        stream_callback.wait_for_delivery().await.unwrap();

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_all_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![caro.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let stream_callback = RustStreamCallback::default();

        let stream = caro
            .conversations()
            .stream_all_messages(Box::new(stream_callback.clone()))
            .await;
        stream.wait_for_ready().await;

        alix_group.send("first".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        let bo_group = bo
            .conversations()
            .create_group(
                vec![caro.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        let _ = caro.inner_client.sync_welcomes().await.unwrap();

        bo_group.send("second".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();
        alix_group.send("third".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();
        bo_group.send("fourth".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        assert_eq!(stream_callback.message_count(), 4);
        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_message_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let amal_group: Arc<FfiGroup> = amal
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bola.inner_client.sync_welcomes().await.unwrap();
        let bola_group = bola.group(amal_group.group_id.clone()).unwrap();

        let stream_callback = RustStreamCallback::default();
        let stream_closer = bola_group.stream(Box::new(stream_callback.clone())).await;

        stream_closer.wait_for_ready().await;

        amal_group.send("hello".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        amal_group
            .send("goodbye".as_bytes().to_vec())
            .await
            .unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);
        stream_closer.end_and_wait().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_message_streaming_when_removed_then_added() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;
        log::info!(
            "Created Inbox IDs {} and {}",
            amal.inbox_id(),
            bola.inbox_id()
        );

        let amal_group = amal
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let stream_callback = RustStreamCallback::default();
        let stream_closer = bola
            .conversations()
            .stream_all_messages(Box::new(stream_callback.clone()))
            .await;
        stream_closer.wait_for_ready().await;

        amal_group.send(b"hello1".to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();
        amal_group.send(b"hello2".to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);
        assert!(!stream_closer.is_closed());

        amal_group
            .remove_members_by_inbox_id(vec![bola.inbox_id().clone()])
            .await
            .unwrap();
        stream_callback.wait_for_delivery().await.unwrap();
        assert_eq!(stream_callback.message_count(), 3); // Member removal transcript message
                                                        //
        amal_group.send(b"hello3".to_vec()).await.unwrap();
        //TODO: could verify with a log message
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive messages while removed
        assert!(!stream_closer.is_closed());

        amal_group
            .add_members(vec![bola.account_address.clone()])
            .await
            .unwrap();

        // TODO: could check for LOG message with a Eviction error on receive
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive transcript messages while removed

        amal_group.send("hello4".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await.unwrap();
        assert_eq!(stream_callback.message_count(), 4); // Receiving messages again
        assert!(!stream_closer.is_closed());

        stream_closer.end_and_wait().await.unwrap();
        assert!(stream_closer.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_who_added_me() {
        // Create Clients
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        // Amal creates a group and adds Bola to the group
        amal.conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
        // and then store that value on the group and insert into the database
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;

        // Bola gets the group id. This will be needed to fetch the group from
        // the database.
        let bola_groups = bola_conversations
            .list(crate::FfiListConversationsOptions {
                created_after_ns: None,
                created_before_ns: None,
                limit: None,
            })
            .await
            .unwrap();

        let bola_group = bola_groups.first().unwrap();

        // Check Bola's group for the added_by_inbox_id of the inviter
        let added_by_inbox_id = bola_group.added_by_inbox_id().unwrap();

        // // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.inbox_id(),
            added_by_inbox_id,
            "The Inviter and added_by_address do not match!"
        );
    }

    // TODO: Test current fails 50% of the time with db locking messages
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_groups_gets_callback_when_streaming_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callback = RustStreamCallback::default();
        let group_callback = RustStreamCallback::default();
        let stream_groups = bo
            .conversations()
            .stream(Box::new(group_callback.clone()))
            .await;

        let stream_messages = bo
            .conversations()
            .stream_all_messages(Box::new(message_callback.clone()))
            .await;
        stream_messages.wait_for_ready().await;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        group_callback.wait_for_delivery().await.unwrap();

        alix_group.send("hello1".as_bytes().to_vec()).await.unwrap();
        message_callback.wait_for_delivery().await.unwrap();

        assert_eq!(group_callback.message_count(), 1);
        assert_eq!(message_callback.message_count(), 1);

        stream_messages.end_and_wait().await.unwrap();
        assert!(stream_messages.is_closed());

        stream_groups.end_and_wait().await.unwrap();
        assert!(stream_groups.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_permissions_show_expected_values() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        // Create admin_only group
        let admin_only_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
            ..Default::default()
        };
        let alix_group_admin_only = alix
            .conversations()
            .create_group(vec![bo.account_address.clone()], admin_only_options)
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_admin_only
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);

        // Create all_members group
        let all_members_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AllMembers),
            ..Default::default()
        };
        let alix_group_all_members = alix
            .conversations()
            .create_group(vec![bo.account_address.clone()], all_members_options)
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_all_members
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Allow,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Allow,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_permissions_updates() {
        let alix = new_test_client().await;
        let bola = new_test_client().await;

        let admin_only_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
            ..Default::default()
        };
        let alix_group = alix
            .conversations()
            .create_group(vec![bola.account_address.clone()], admin_only_options)
            .await
            .unwrap();

        let alix_group_permissions = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_group_permissions, expected_permission_policy_set);

        // Let's update the group so that the image url can be updated by anyone
        alix_group
            .update_permission_policy(
                FfiPermissionUpdateType::UpdateMetadata,
                FfiPermissionPolicy::Allow,
                Some(FfiMetadataField::ImageUrlSquare),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();
        let alix_group_permissions = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let new_expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_group_permissions, new_expected_permission_policy_set);

        // Verify that bo can not update the group name
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;
        let bola_groups = bola_conversations
            .list(crate::FfiListConversationsOptions {
                created_after_ns: None,
                created_before_ns: None,
                limit: None,
            })
            .await
            .unwrap();

        let bola_group = bola_groups.first().unwrap();
        bola_group
            .update_group_name("new_name".to_string())
            .await
            .unwrap_err();

        // Verify that bo CAN update the image url
        bola_group
            .update_group_image_url_square("https://example.com/image.png".to_string())
            .await
            .unwrap();

        // Verify we can read the correct values from the group
        bola_group.sync().await.unwrap();
        alix_group.sync().await.unwrap();
        assert_eq!(
            bola_group.group_image_url_square().unwrap(),
            "https://example.com/image.png"
        );
        assert_eq!(bola_group.group_name().unwrap(), "");
        assert_eq!(
            alix_group.group_image_url_square().unwrap(),
            "https://example.com/image.png"
        );
        assert_eq!(alix_group.group_name().unwrap(), "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_group_creation_custom_permissions() {
        let alix = new_test_client().await;
        let bola = new_test_client().await;

        let custom_permissions = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Admin,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
        };

        let create_group_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            group_pinned_frame_url: Some("https://example.com/frame.png".to_string()),
            custom_permission_policy_set: Some(custom_permissions),
        };

        let alix_group = alix
            .conversations()
            .create_group(vec![bola.account_address.clone()], create_group_options)
            .await
            .unwrap();

        // Verify the group was created with the correct permissions
        let group_permissions_policy_set = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        assert_eq!(
            group_permissions_policy_set.add_admin_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.remove_admin_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.update_group_name_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.update_group_description_policy,
            FfiPermissionPolicy::Allow
        );
        assert_eq!(
            group_permissions_policy_set.update_group_image_url_square_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.update_group_pinned_frame_url_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.add_member_policy,
            FfiPermissionPolicy::Allow
        );
        assert_eq!(
            group_permissions_policy_set.remove_member_policy,
            FfiPermissionPolicy::Deny
        );

        // Verify that Bola can not update the group name
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;
        let bola_groups = bola_conversations
            .list(crate::FfiListConversationsOptions {
                created_after_ns: None,
                created_before_ns: None,
                limit: None,
            })
            .await
            .unwrap();

        let bola_group = bola_groups.first().unwrap();
        bola_group
            .update_group_name("new_name".to_string())
            .await
            .unwrap_err();
        let result = bola_group
            .update_group_name("New Group Name".to_string())
            .await;
        assert!(result.is_err());

        // Verify that Alix can update the group name
        let result = alix_group
            .update_group_name("New Group Name".to_string())
            .await;
        assert!(result.is_ok());

        // Verify that Bola can update the group description
        let result = bola_group
            .update_group_description("New Description".to_string())
            .await;
        assert!(result.is_ok());

        // Verify that Alix can not remove bola even though they are a super admin
        let result = alix_group
            .remove_members(vec![bola.account_address.clone()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_group_creation_custom_permissions_fails_when_invalid() {
        let alix = new_test_client().await;
        let bola = new_test_client().await;

        // Add / Remove Admin must be Admin or Super Admin or Deny
        let custom_permissions_invalid_1 = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Allow,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
        };

        let custom_permissions_valid = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Admin,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_group_pinned_frame_url_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
        };

        let create_group_options_invalid_1 = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            group_pinned_frame_url: Some("https://example.com/frame.png".to_string()),
            custom_permission_policy_set: Some(custom_permissions_invalid_1),
        };

        let results_1 = alix
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                create_group_options_invalid_1,
            )
            .await;

        assert!(results_1.is_err());

        let create_group_options_invalid_2 = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AllMembers),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            group_pinned_frame_url: Some("https://example.com/frame.png".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid.clone()),
        };

        let results_2 = alix
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                create_group_options_invalid_2,
            )
            .await;

        assert!(results_2.is_err());

        let create_group_options_invalid_3 = FfiCreateGroupOptions {
            permissions: None,
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            group_pinned_frame_url: Some("https://example.com/frame.png".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid.clone()),
        };

        let results_3 = alix
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                create_group_options_invalid_3,
            )
            .await;

        assert!(results_3.is_err());

        let create_group_options_valid = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            group_pinned_frame_url: Some("https://example.com/frame.png".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid),
        };

        let results_4 = alix
            .conversations()
            .create_group(
                vec![bola.account_address.clone()],
                create_group_options_valid,
            )
            .await;

        assert!(results_4.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_revoke_all_installations() {
        let wallet = xmtp_cryptography::utils::LocalWallet::new(&mut rng());
        let client_1 = new_test_client_with_wallet(wallet.clone()).await;
        let client_2 = new_test_client_with_wallet(wallet.clone()).await;

        let client_1_state = client_1.inbox_state(true).await.unwrap();
        let client_2_state = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state.installations.len(), 2);
        assert_eq!(client_2_state.installations.len(), 2);

        let signature_request = client_1.revoke_all_other_installations().await.unwrap();
        sign_with_wallet(&wallet, &signature_request).await;
        client_1
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
        let client_2_state_after_revoke = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state_after_revoke.installations.len(), 1);
        assert_eq!(client_2_state_after_revoke.installations.len(), 1);
        assert_eq!(
            client_1_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
        assert_eq!(
            client_2_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dms_sync_but_do_not_list() {
        let alix = new_test_client().await;
        let bola = new_test_client().await;

        let alix_conversations = alix.conversations();
        let bola_conversations = bola.conversations();

        let _alix_group = alix_conversations
            .create_dm(bola.account_address.clone())
            .await
            .unwrap();
        let alix_num_sync = alix_conversations.sync_all_groups().await.unwrap();
        bola_conversations.sync().await.unwrap();
        let bola_num_sync = bola_conversations.sync_all_groups().await.unwrap();
        assert_eq!(alix_num_sync, 1);
        assert_eq!(bola_num_sync, 1);

        let alix_groups = alix_conversations
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_groups.len(), 0);

        let bola_groups = bola_conversations
            .list(FfiListConversationsOptions::default())
            .await
            .unwrap();
        assert_eq!(bola_groups.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_and_get_group_consent() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let alix_initial_consent = alix_group.consent_state().unwrap();
        assert_eq!(alix_initial_consent, FfiConsentState::Allowed);

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.group(alix_group.id()).unwrap();

        let bo_initial_consent = bo_group.consent_state().unwrap();
        assert_eq!(bo_initial_consent, FfiConsentState::Unknown);

        alix_group
            .update_consent_state(FfiConsentState::Denied)
            .unwrap();
        let alix_updated_consent = alix_group.consent_state().unwrap();
        assert_eq!(alix_updated_consent, FfiConsentState::Denied);
        bo.set_consent_states(vec![FfiConsent {
            state: FfiConsentState::Allowed,
            entity_type: FfiConsentEntityType::GroupId,
            entity: hex::encode(bo_group.id()),
        }])
        .await
        .unwrap();
        let bo_updated_consent = bo_group.consent_state().unwrap();
        assert_eq!(bo_updated_consent, FfiConsentState::Allowed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_and_get_member_consent() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        alix.set_consent_states(vec![FfiConsent {
            state: FfiConsentState::Allowed,
            entity_type: FfiConsentEntityType::Address,
            entity: bo.account_address.clone(),
        }])
        .await
        .unwrap();
        let bo_consent = alix
            .get_consent_state(FfiConsentEntityType::Address, bo.account_address.clone())
            .await
            .unwrap();
        assert_eq!(bo_consent, FfiConsentState::Allowed);

        if let Some(member) = alix_group
            .list_members()
            .await
            .unwrap()
            .iter()
            .find(|&m| m.inbox_id == bo.inbox_id())
        {
            assert_eq!(member.consent_state, FfiConsentState::Allowed);
        } else {
            panic!("Error: No member found with the given inbox_id.");
        }
    }
}
