pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::logger::FfiLogger;
use crate::GenericError;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::{sync::Mutex, task::AbortHandle};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::{
    associations::{
        builder::SignatureRequest, generate_inbox_id as xmtp_id_generate_inbox_id,
        RecoverableEcdsaSignature, SmartContractWalletSignature,
    },
    InboxId,
};
use xmtp_mls::groups::group_mutable_metadata::MetadataField;
use xmtp_mls::groups::group_permissions::BasePolicies;
use xmtp_mls::groups::group_permissions::GroupMutablePermissionsError;
use xmtp_mls::groups::group_permissions::MembershipPolicies;
use xmtp_mls::groups::group_permissions::MetadataBasePolicies;
use xmtp_mls::groups::group_permissions::MetadataPolicies;
use xmtp_mls::groups::group_permissions::PermissionsBasePolicies;
use xmtp_mls::groups::group_permissions::PermissionsPolicies;
use xmtp_mls::groups::intents::PermissionPolicyOption;
use xmtp_mls::groups::intents::PermissionUpdateType;
use xmtp_mls::groups::GroupMetadataOptions;
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
#[allow(unused)]
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
        "Creating message store with path: {:?} and encryption key: {}",
        db,
        encryption_key.is_some()
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
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSignatureRequest {
    // Signature that's signed by EOA wallet
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        let signature_text = inner.signature_text();
        inner
            .add_signature(Box::new(RecoverableEcdsaSignature::new(
                signature_text,
                signature_bytes,
            )))
            .await?;

        Ok(())
    }

    // Signature that's signed by smart contract wallet
    pub async fn add_scw_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_rpc_url: String,
    ) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        let signature = SmartContractWalletSignature::new_with_rpc(
            inner.signature_text(),
            signature_bytes,
            address,
            chain_rpc_url,
        )
        .await?;
        inner.add_signature(Box::new(signature)).await?;
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
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn signature_request(&self) -> Option<Arc<FfiSignatureRequest>> {
        self.inner_client
            .identity()
            .signature_request()
            .map(|request| {
                Arc::new(FfiSignatureRequest {
                    inner: Arc::new(Mutex::new(request)),
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
        self.inner_client.send_history_request().await?;
        Ok(())
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

#[derive(uniffi::Enum, Debug)]
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

#[derive(uniffi::Enum, Debug, PartialEq, Eq)]
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

#[derive(uniffi::Record, Debug, PartialEq, Eq)]
pub struct FfiPermissionPolicySet {
    pub add_member_policy: FfiPermissionPolicy,
    pub remove_member_policy: FfiPermissionPolicy,
    pub add_admin_policy: FfiPermissionPolicy,
    pub remove_admin_policy: FfiPermissionPolicy,
    pub update_group_name_policy: FfiPermissionPolicy,
    pub update_group_description_policy: FfiPermissionPolicy,
    pub update_group_image_url_square_policy: FfiPermissionPolicy,
}

impl From<PreconfiguredPolicies> for FfiGroupPermissionsOptions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::AllMembers => FfiGroupPermissionsOptions::AllMembers,
            PreconfiguredPolicies::AdminsOnly => FfiGroupPermissionsOptions::AdminOnly,
        }
    }
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiMetadataField {
    GroupName,
    Description,
    ImageUrlSquare,
}

impl From<&FfiMetadataField> for MetadataField {
    fn from(field: &FfiMetadataField) -> Self {
        match field {
            FfiMetadataField::GroupName => MetadataField::GroupName,
            FfiMetadataField::Description => MetadataField::Description,
            FfiMetadataField::ImageUrlSquare => MetadataField::GroupImageUrlSquare,
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

        let group_permissions = match opts.permissions {
            Some(FfiGroupPermissionsOptions::AllMembers) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AllMembers)
            }
            Some(FfiGroupPermissionsOptions::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly)
            }
            _ => None,
        };

        let convo = self
            .inner_client
            .create_group(group_permissions, opts.into_group_metadata_options())?;
        if !account_addresses.is_empty() {
            convo
                .add_members(&self.inner_client, account_addresses)
                .await?;
        }

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

    pub async fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiGroup>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiGroup>> = inner
            .find_groups(
                None,
                opts.created_after_ns,
                opts.created_before_ns,
                opts.limit,
            )?
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
        let handle =
            RustXmtpClient::stream_conversations_with_callback(client.clone(), move |convo| {
                callback.on_conversation(Arc::new(FfiGroup {
                    inner_client: client.clone(),
                    group_id: convo.group_id,
                    created_at_ns: convo.created_at_ns,
                }))
            });

        FfiStreamCloser::new(handle)
    }

    pub fn stream_all_messages(
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
}

#[derive(uniffi::Enum)]
pub enum FfiPermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
}

#[derive(uniffi::Record, Default)]
pub struct FfiCreateGroupOptions {
    pub permissions: Option<FfiGroupPermissionsOptions>,
    pub group_name: Option<String>,
    pub group_image_url_square: Option<String>,
    pub group_description: Option<String>,
}

impl FfiCreateGroupOptions {
    pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
        GroupMetadataOptions {
            name: self.group_name,
            image_url_square: self.group_image_url_square,
            description: self.group_description,
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

        log::debug!("Sending message");
        let message_id = group
            .send_message(content_bytes.as_slice(), &self.inner_client)
            .await?;
        Ok(message_id)
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
            .process_streamed_group_message(envelope_bytes, self.inner_client.clone())
            .await?;
        let ffi_message = message.into();

        Ok(ffi_message)
    }

    pub fn list_members(&self) -> Result<Vec<FfiGroupMember>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let members: Vec<FfiGroupMember> = group
            .members()?
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

        let group_name = group.group_name()?;

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

        let group_image_url_square = group.group_image_url_square()?;

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

        let group_description = group.group_description()?;

        Ok(group_description)
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let admin_list = group.admin_list()?;

        Ok(admin_list)
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let super_admin_list = group.super_admin_list()?;

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

        Ok(group.is_active()?)
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

        let metadata = group.metadata()?;
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

#[derive(uniffi::Enum)]
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

#[uniffi::export]
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
            let join_result = h.handle.await;
            if matches!(join_result, Err(ref e) if !e.is_cancelled()) {
                return Err(GenericError::Generic {
                    err: format!(
                        "subscription event loop join error {}",
                        join_result.unwrap_err()
                    ),
                });
            }
        } else {
            log::warn!("subscription already closed");
        }
        Ok(())
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
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        get_inbox_id_for_address, inbox_owner::SigningError, logger::FfiLogger,
        FfiConversationCallback, FfiCreateGroupOptions, FfiGroup, FfiGroupPermissionsOptions,
        FfiInboxOwner, FfiListConversationsOptions, FfiListMessagesOptions, FfiMetadataField,
        FfiPermissionPolicy, FfiPermissionPolicySet, FfiPermissionUpdateType,
    };
    use std::{
        env,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc, Mutex,
        },
    };

    use super::{create_client, FfiMessage, FfiMessageCallback, FfiXmtpClient};
    use ethers::utils::hex;
    use ethers_core::rand::{
        self,
        distributions::{Alphanumeric, DistString},
    };
    use xmtp_cryptography::{signature::RecoverableSignature, utils::rng};
    use xmtp_id::associations::generate_inbox_id;
    use xmtp_mls::{storage::EncryptionKey, InboxOwner};

    #[derive(Clone)]
    pub struct LocalWalletInboxOwner {
        wallet: xmtp_cryptography::utils::LocalWallet,
    }

    impl LocalWalletInboxOwner {
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
            println!("[{}][t:{}]: {}", level_label, thread_id::get(), message)
        }
    }

    #[derive(Default, Clone)]
    struct RustStreamCallback {
        num_messages: Arc<AtomicU32>,
        messages: Arc<Mutex<Vec<FfiMessage>>>,
        conversations: Arc<Mutex<Vec<Arc<FfiGroup>>>>,
        notify: Arc<tokio::sync::Notify>,
    }

    impl RustStreamCallback {
        pub fn message_count(&self) -> u32 {
            self.num_messages.load(Ordering::SeqCst)
        }

        pub async fn wait_for_delivery(&self) {
            self.notify.notified().await
        }
    }

    impl FfiMessageCallback for RustStreamCallback {
        fn on_message(&self, message: FfiMessage) {
            log::debug!("On message called");
            let mut messages = self.messages.lock().unwrap();
            log::info!("Received: {}", String::from_utf8_lossy(&message.content));
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

    async fn new_test_client() -> Arc<FfiXmtpClient> {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let nonce = 1;
        let inbox_id = generate_inbox_id(&ffi_inbox_owner.get_address(), &nonce);

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
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

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let client = new_test_client().await;
        assert!(client.signature_request().is_some());
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

        let members = group.list_members().unwrap();
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
                },
            )
            .await
            .unwrap();

        let members = group.list_members().unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(group.group_name().unwrap(), "Group Name");
        assert_eq!(group.group_image_url_square().unwrap(), "url");
        assert_eq!(group.group_description().unwrap(), "group description");
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

    // Looks like this test might be a separate issue
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_group_messages_for_updates() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callbacks = RustStreamCallback::default();
        let stream_messages = bo
            .conversations()
            .stream_all_messages(Box::new(message_callbacks.clone()));
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
        message_callbacks.wait_for_delivery().await;

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
        message_callbacks.wait_for_delivery().await;

        // Uncomment the following lines to add more group name updates
        bo_group
            .update_group_name("Old Name3".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery().await;

        assert_eq!(message_callbacks.message_count(), 3);

        stream_messages.end_and_wait().await.unwrap();

        assert!(stream_messages.is_closed());
    }

    // test is also showing intermittent failures with database locked msg
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_and_update_name_without_forking_group() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callbacks = RustStreamCallback::default();
        let stream_messages = bo
            .conversations()
            .stream_all_messages(Box::new(message_callbacks.clone()));
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
        message_callbacks.wait_for_delivery().await;
        alix_group.send("hello1".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await;

        bo.conversations().sync().await.unwrap();

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
        message_callbacks.wait_for_delivery().await;
        bo_group.send("hello3".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await;

        alix_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(alix_messages.len(), second_msg_check);

        alix_group.send("hello4".as_bytes().to_vec()).await.unwrap();
        message_callbacks.wait_for_delivery().await;
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

        stream_callback.wait_for_delivery().await;

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(
                vec![bola.account_address.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        stream_callback.wait_for_delivery().await;

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
            .stream_all_messages(Box::new(stream_callback.clone()));
        stream.wait_for_ready().await;

        alix_group.send("first".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;

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
        stream_callback.wait_for_delivery().await;
        alix_group.send("third".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;
        bo_group.send("fourth".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;

        assert_eq!(stream_callback.message_count(), 4);
        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_message_streaming() {
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

        let stream_callback = RustStreamCallback::default();
        let stream_closer = group.stream(Box::new(stream_callback.clone())).await;

        group.send("hello".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;
        group.send("goodbye".as_bytes().to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;

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
            .stream_all_messages(Box::new(stream_callback.clone()));
        stream_closer.wait_for_ready().await;

        amal_group.send(b"hello1".to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;
        amal_group.send(b"hello2".to_vec()).await.unwrap();
        stream_callback.wait_for_delivery().await;

        assert_eq!(stream_callback.message_count(), 2);
        assert!(!stream_closer.is_closed());

        amal_group
            .remove_members_by_inbox_id(vec![bola.inbox_id().clone()])
            .await
            .unwrap();
        stream_callback.wait_for_delivery().await;
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
        stream_callback.wait_for_delivery().await;
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
            .stream_all_messages(Box::new(message_callback.clone()));
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
        group_callback.wait_for_delivery().await;

        alix_group.send("hello1".as_bytes().to_vec()).await.unwrap();
        message_callback.wait_for_delivery().await;

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
}
