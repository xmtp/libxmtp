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

pub type RustXmtpClient = MlsClient<ApiDebugWrapper<TonicApiClient>>;

pub struct FfiConversations {
    inner_client: Arc<RustXmtpClient>,
}

impl FfiConversations {
    pub fn create_group_optimistic(
        &self,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn create_group(
        &self,
        account_identities: Vec<FfiIdentifier>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn create_group_with_inbox_ids(
        &self,
        inbox_ids: Vec<String>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn find_or_create_dm(
        &self,
        target_identity: FfiIdentifier,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn find_or_create_dm_by_inbox_id(
        &self,
        inbox_id: String,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        // ... existing code ...
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn sync_all_conversations(
        &self,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> Result<u32, GenericError> {
        // ... existing code ...
    }

    pub fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        // ... existing code ...
    }

    pub fn list_groups(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        // ... existing code ...
    }

    pub fn list_dms(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        // ... existing code ...
    }

    pub async fn stream_groups(
        &self,
        callback: Arc<dyn FfiConversationCallback>,
    ) -> FfiStreamCloser {
        // ... existing code ...
    }

    pub async fn stream_dms(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        // ... existing code ...
    }

    pub async fn stream(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        // ... existing code ...
    }
}

pub struct FfiConversation {
    inner: MlsGroup<RustXmtpClient>,
}

impl FfiConversation {
    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        // ... existing code ...
    }

    pub(crate) async fn send_text(&self, text: &str) -> Result<Vec<u8>, GenericError> {
        // ... existing code ...
    }

    pub fn send_optimistic(&self, content_bytes: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        // ... existing code ...
    }

    pub async fn publish_messages(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        // ... existing code ...
    }

    pub async fn find_messages_with_reactions(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessageWithReactions>, GenericError> {
        // ... existing code ...
    }

    pub async fn process_streamed_conversation_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<FfiMessage, FfiSubscribeError> {
        // ... existing code ...
    }

    pub async fn list_members(&self) -> Result<Vec<FfiConversationMember>, GenericError> {
        // ... existing code ...
    }

    pub async fn add_members(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<FfiUpdateGroupMembershipResult, GenericError> {
        // ... existing code ...
    }

    pub async fn add_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<FfiUpdateGroupMembershipResult, GenericError> {
        // ... existing code ...
    }

    pub async fn remove_members(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn remove_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn update_group_name(&self, group_name: String) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn group_name(&self) -> Result<String, GenericError> {
        // ... existing code ...
    }

    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn group_image_url_square(&self) -> Result<String, GenericError> {
        // ... existing code ...
    }

    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn group_description(&self) -> Result<String, GenericError> {
        // ... existing code ...
    }

    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: FfiMessageDisappearingSettings,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn remove_conversation_message_disappearing_settings(
        &self,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn conversation_message_disappearing_settings(
        &self,
    ) -> Result<Option<FfiMessageDisappearingSettings>, GenericError> {
        // ... existing code ...
    }

    pub fn is_conversation_message_disappearing_enabled(&self) -> Result<bool, GenericError> {
        // ... existing code ...
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GenericError> {
        // ... existing code ...
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GenericError> {
        // ... existing code ...
    }

    pub fn is_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        // ... existing code ...
    }

    pub fn is_super_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        // ... existing code ...
    }

    pub async fn add_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn remove_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn group_permissions(&self) -> Result<Arc<FfiGroupPermissions>, GenericError> {
        // ... existing code ...
    }

    pub async fn update_permission_policy(
        &self,
        permission_update_type: FfiPermissionUpdateType,
        permission_policy_option: FfiPermissionPolicy,
        metadata_field: Option<FfiMetadataField>,
    ) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub async fn stream(&self, message_callback: Arc<dyn FfiMessageCallback>) -> FfiStreamCloser {
        // ... existing code ...
    }

    pub fn created_at_ns(&self) -> i64 {
        // ... existing code ...
    }

    pub fn is_active(&self) -> Result<bool, GenericError> {
        // ... existing code ...
    }

    pub fn paused_for_version(&self) -> Result<Option<String>, GenericError> {
        // ... existing code ...
    }

    pub fn consent_state(&self) -> Result<FfiConsentState, GenericError> {
        // ... existing code ...
    }

    pub fn update_consent_state(&self, state: FfiConsentState) -> Result<(), GenericError> {
        // ... existing code ...
    }

    pub fn added_by_inbox_id(&self) -> Result<String, GenericError> {
        // ... existing code ...
    }

    pub async fn group_metadata(&self) -> Result<Arc<FfiConversationMetadata>, GenericError> {
        // ... existing code ...
    }

    pub fn dm_peer_inbox_id(&self) -> Option<String> {
        // ... existing code ...
    }

    pub fn get_hmac_keys(&self) -> Result<HashMap<Vec<u8>, Vec<FfiHmacKey>>, GenericError> {
        // ... existing code ...
    }

    pub async fn conversation_type(&self) -> Result<FfiConversationType, GenericError> {
        // ... existing code ...
    }

    pub async fn conversation_debug_info(&self) -> Result<FfiConversationDebugInfo, GenericError> {
        // ... existing code ...
    }

    pub async fn find_duplicate_dms(&self) -> Result<Vec<Arc<FfiConversation>>, GenericError> {
        // ... existing code ...
    }

    pub fn id(&self) -> Vec<u8> {
        // ... existing code ...
    }
}

pub struct FfiConversationListItem {
    conversation: FfiConversation,
    last_message: Option<FfiMessage>,
}

impl FfiConversationListItem {
    pub fn conversation(&self) -> Arc<FfiConversation> {
        // ... existing code ...
    }

    pub fn last_message(&self) -> Option<FfiMessage> {
        // ... existing code ...
    }
}

pub struct FfiUpdateGroupMembershipResult {
    added_members: HashMap<String, u64>,
    removed_members: Vec<String>,
    failed_installations: Vec<Vec<u8>>,
}

impl FfiUpdateGroupMembershipResult {
    fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        // ... existing code ...
    }
}

pub struct FfiMessageDisappearingSettings {
    pub from_ns: i64,
    pub in_ns: i64,
}

impl FfiMessageDisappearingSettings {
    fn new(from_ns: i64, in_ns: i64) -> Self {
        // ... existing code ...
    }
}

pub struct FfiConversationDebugInfo {
    pub epoch: u64,
    pub maybe_forked: bool,
    pub fork_details: String,
}

impl FfiConversationDebugInfo {
    fn new(epoch: u64, maybe_forked: bool, fork_details: String) -> Self {
        // ... existing code ...
    }
}

pub struct FfiConversationMetadata {
    inner: Arc<GroupMetadata>,
}

impl FfiConversationMetadata {
    pub fn creator_inbox_id(&self) -> String {
        // ... existing code ...
    }

    pub fn conversation_type(&self) -> FfiConversationType {
        // ... existing code ...
    }
}

pub struct FfiGroupPermissions {
    inner: Arc<GroupMutablePermissions>,
}

impl FfiGroupPermissions {
    pub fn policy_type(&self) -> Result<FfiGroupPermissionsOptions, GenericError> {
        // ... existing code ...
    }

    pub fn policy_set(&self) -> Result<FfiPermissionPolicySet, GenericError> {
        // ... existing code ...
    }
}

pub enum FfiConversationMessageKind {
    Application,
    MembershipChange,
}

pub enum FfiConversationType {
    Group,
    Dm,
    Sync,
}

pub enum FfiGroupPermissionsOptions {
    Default,
    AdminOnly,
    CustomPolicy,
}

pub enum FfiPermissionUpdateType {
    AddMember,
    RemoveMember,
    AddAdmin,
    RemoveAdmin,
    UpdateMetadata,
}

pub enum FfiPermissionPolicy {
    Allow,
    Deny,
    Admin,
    SuperAdmin,
    DoesNotExist,
    Other,
}

pub enum FfiMetadataField {
    GroupName,
    Description,
    ImageUrlSquare,
}

pub struct FfiPermissionPolicySet {
    pub add_member_policy: FfiPermissionPolicy,
    pub remove_member_policy: FfiPermissionPolicy,
    pub add_admin_policy: FfiPermissionPolicy,
    pub remove_admin_policy: FfiPermissionPolicy,
    pub update_group_name_policy: FfiPermissionPolicy,
    pub update_group_description_policy: FfiPermissionPolicy,
    pub update_group_image_url_square_policy: FfiPermissionPolicy,
    pub update_message_disappearing_policy: FfiPermissionPolicy,
}

pub struct FfiCreateGroupOptions {
    pub permissions: Option<FfiGroupPermissionsOptions>,
    pub group_name: Option<String>,
    pub group_image_url_square: Option<String>,
    pub group_description: Option<String>,
    pub custom_permission_policy_set: Option<FfiPermissionPolicySet>,
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
}

impl FfiCreateGroupOptions {
    pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
        // ... existing code ...
    }
}

pub struct FfiCreateDMOptions {
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
}

impl FfiCreateDMOptions {
    pub fn new(disappearing_settings: FfiMessageDisappearingSettings) -> Self {
        // ... existing code ...
    }

    pub fn into_dm_metadata_options(self) -> DMMetadataOptions {
        // ... existing code ...
    }
}

pub struct FfiListConversationsOptions {
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub limit: Option<i64>,
    pub consent_states: Option<Vec<FfiConsentState>>,
    pub include_duplicate_dms: bool,
}

pub struct FfiConversationMember {
    pub inbox_id: String,
    pub account_identifiers: Vec<FfiIdentifier>,
    pub installation_ids: Vec<Vec<u8>>,
    pub permission_level: FfiPermissionLevel,
    pub consent_state: FfiConsentState,
}

pub enum FfiPermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use xmtp_cryptography::utils::LocalWallet;

    #[tokio::test]
    async fn test_create_conversation() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 2);
    }

    #[tokio::test]
    async fn test_create_conversation_with_self() {
        let client = new_test_client().await;

        let conversation = client
            .create_conversation(vec![client.account_identifier.clone()])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 1);
    }

    #[tokio::test]
    async fn test_create_conversation_with_multiple_participants() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;
        let client_c = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![
                client_b.account_identifier.clone(),
                client_c.account_identifier.clone(),
            ])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 3);
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        let conversations = client_a.list_conversations().await.unwrap();

        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].id(), conversation.id());
    }

    #[tokio::test]
    async fn test_get_conversation() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        let retrieved = client_a.get_conversation(conversation.id()).await.unwrap();

        assert_eq!(retrieved.id(), conversation.id());
    }

    #[tokio::test]
    async fn test_get_conversation_not_found() {
        let client = new_test_client().await;

        let result = client.get_conversation("nonexistent".to_string()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_participants() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;
        let client_c = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        conversation
            .add_participants(vec![client_c.account_identifier.clone()])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 3);
    }

    #[tokio::test]
    async fn test_remove_participants() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;
        let client_c = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![
                client_b.account_identifier.clone(),
                client_c.account_identifier.clone(),
            ])
            .await
            .unwrap();

        conversation
            .remove_participants(vec![client_c.account_identifier.clone()])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 2);
    }

    #[tokio::test]
    async fn test_remove_all_participants() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;
        let client_c = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![
                client_b.account_identifier.clone(),
                client_c.account_identifier.clone(),
            ])
            .await
            .unwrap();

        conversation
            .remove_participants(vec![
                client_b.account_identifier.clone(),
                client_c.account_identifier.clone(),
            ])
            .await
            .unwrap();

        assert_eq!(conversation.participants().len(), 1);
    }
} 