//! Plain records, enums, and their conversions.

use super::*;

use crate::FfiError;
use crate::identity::FfiIdentifier;
use crate::message::FfiDeliveryStatus;

use std::{collections::HashMap, convert::TryInto, sync::Arc};
use xmtp_common::time::now_ns;

use xmtp_db::group::{ConversationType, GroupMembershipState, GroupQueryOrderBy};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::group_message::{SortBy, SortDirection};
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    group_message::{GroupMessageKind, StoredGroupMessage},
};
use xmtp_id::associations::ident;
use xmtp_id::key_package::{VerifiedKeyPackageV2, VerifiedLifetime};

use xmtp_id::associations::{AssociationState, MemberIdentifier};

use xmtp_mls::groups::{
    ConversationDebugInfo, GroupMembershipCapabilities, InboxCapabilities,
    InstallationCapabilities, MlsExtensionType,
};
use xmtp_mls::groups::{
    PreconfiguredPolicies,
    group_permissions::{
        BasePolicies, GroupMutablePermissions, GroupMutablePermissionsError, MembershipPolicies,
        MetadataBasePolicies, MetadataPolicies, PermissionsBasePolicies, PermissionsPolicies,
        PolicySet,
    },
    intents::{PermissionPolicyOption, PermissionUpdateType},
};
use xmtp_mls::mls_common::group_metadata::GroupMetadata;
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;

use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::types::Cursor;

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FfiGroupSyncSummary {
    pub num_eligible: u64,
    pub num_synced: u64,
}

impl From<xmtp_mls::groups::welcome_sync::GroupSyncSummary> for FfiGroupSyncSummary {
    fn from(summary: xmtp_mls::groups::welcome_sync::GroupSyncSummary) -> Self {
        Self {
            num_eligible: summary.num_eligible as u64,
            num_synced: summary.num_synced as u64,
        }
    }
}

/// Options for [`FfiXmtpClient::catch_up_to_live`]. Taken as a struct (rather
/// than a bare argument) so future knobs can be added as defaulted fields
/// without breaking the exported signature — same pattern as
/// [`FfiUpdateAppDataOptions`].
///
/// WARNING: uniffi Records get NO default field values unless the field
/// carries `#[uniffi(default = ...)]`. Any field added later MUST carry a
/// uniffi default, or the generated Swift/Kotlin constructors change and the
/// addition breaks compiled apps.
#[derive(uniffi::Record, Default, Clone, Debug)]
pub struct FfiCatchUpOptions {
    /// Catch-up deadline in milliseconds. None uses the client's barrier timeout.
    /// A deadline error retains partial committed counts and unfinished target details.
    #[uniffi(default = None)]
    pub timeout_ms: Option<u64>,
}

/// Outcome of [`FfiXmtpClient::catch_up_to_live`].
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FfiCatchUpSummary {
    /// Newly deliverable retained rows from this run, including partial failed runs.
    pub messages: u64,
    /// Conversations newly joined by this run.
    pub conversations: u64,
    /// True only when every fixed processing target completed.
    /// Failed runs expose a false summary through structured error details.
    pub completed: bool,
    /// Enrolled groups with a newly recorded terminal rejection during this run.
    pub failed: u64,
}

impl From<xmtp_mls::subscriptions::catch_up::CatchUpSummary> for FfiCatchUpSummary {
    fn from(summary: xmtp_mls::subscriptions::catch_up::CatchUpSummary) -> Self {
        Self {
            messages: summary.messages,
            conversations: summary.conversations,
            completed: summary.completed,
            failed: summary.failed,
        }
    }
}

impl From<HmacKey> for FfiHmacKey {
    fn from(value: HmacKey) -> Self {
        Self {
            epoch: value.epoch,
            key: value.key.to_vec(),
        }
    }
}

/// Timeout for `wait_for_registration_visible`.
#[derive(uniffi::Record, Default)]
pub struct FfiVisibilityConfirmationOptions {
    /// Maximum wait time in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl From<FfiVisibilityConfirmationOptions> for xmtp_mls::client::VisibilityConfirmationOptions {
    fn from(opts: FfiVisibilityConfirmationOptions) -> Self {
        Self {
            timeout_ms: opts.timeout_ms.unwrap_or(Self::default().timeout_ms),
        }
    }
}

/// Signature kind used in identity operations
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum FfiSignatureKind {
    /// ERC-191 signature (Externally Owned Account/EOA)
    Erc191,
    /// ERC-1271 signature (Smart Contract Wallet/SCW)
    Erc1271,
    /// Installation key signature
    InstallationKey,
    /// Legacy delegated signature
    LegacyDelegated,
    /// P256 passkey signature
    P256,
}

impl From<xmtp_id::associations::SignatureKind> for FfiSignatureKind {
    fn from(kind: xmtp_id::associations::SignatureKind) -> Self {
        match kind {
            xmtp_id::associations::SignatureKind::Erc191 => FfiSignatureKind::Erc191,
            xmtp_id::associations::SignatureKind::Erc1271 => FfiSignatureKind::Erc1271,
            xmtp_id::associations::SignatureKind::InstallationKey => {
                FfiSignatureKind::InstallationKey
            }
            xmtp_id::associations::SignatureKind::LegacyDelegated => {
                FfiSignatureKind::LegacyDelegated
            }
            xmtp_id::associations::SignatureKind::P256 => FfiSignatureKind::P256,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiInboxState {
    pub inbox_id: String,
    pub recovery_identity: FfiIdentifier,
    pub installations: Vec<FfiInstallation>,
    pub account_identities: Vec<FfiIdentifier>,
    pub creation_signature_kind: Option<FfiSignatureKind>,
}

#[derive(uniffi::Record)]
pub struct FfiHmacKey {
    pub(crate) key: Vec<u8>,
    pub(crate) epoch: i64,
}

#[derive(uniffi::Record)]
pub struct FfiInstallation {
    pub id: Vec<u8>,
    pub client_timestamp_ns: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct FfiKeyPackageStatus {
    pub lifetime: Option<FfiLifetime>,
    pub validation_error: Option<String>,
}

#[derive(uniffi::Record)]
pub struct FfiLifetime {
    pub not_before: u64,
    pub not_after: u64,
}

impl From<VerifiedLifetime> for FfiLifetime {
    fn from(value: VerifiedLifetime) -> Self {
        Self {
            not_before: value.not_before,
            not_after: value.not_after,
        }
    }
}

impl From<VerifiedKeyPackageV2> for FfiKeyPackageStatus {
    fn from(value: VerifiedKeyPackageV2) -> Self {
        Self {
            lifetime: value.life_time().map(Into::into),
            validation_error: None,
        }
    }
}

impl From<AssociationState> for FfiInboxState {
    fn from(state: AssociationState) -> Self {
        Self {
            inbox_id: state.inbox_id().to_string(),
            recovery_identity: state.recovery_identifier().clone().into(),
            installations: state
                .members()
                .into_iter()
                .filter_map(|m| match m.identifier {
                    MemberIdentifier::Ethereum(_) => None,
                    MemberIdentifier::Passkey(_) => None,
                    MemberIdentifier::Installation(ident::Installation(id)) => {
                        Some(FfiInstallation {
                            id,
                            client_timestamp_ns: m.client_timestamp_ns,
                        })
                    }
                })
                .collect(),
            account_identities: state.identifiers().into_iter().map(Into::into).collect(),
            creation_signature_kind: None, // Will be populated by inbox_state method
        }
    }
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiGroupQueryOrderBy {
    CreatedAt,
    LastActivity,
}

impl From<FfiGroupQueryOrderBy> for GroupQueryOrderBy {
    fn from(order_by: FfiGroupQueryOrderBy) -> Self {
        match order_by {
            FfiGroupQueryOrderBy::CreatedAt => GroupQueryOrderBy::CreatedAt,
            FfiGroupQueryOrderBy::LastActivity => GroupQueryOrderBy::LastActivity,
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiSendMessageOpts {
    pub should_push: bool,
    /// Optional idempotency key. Re-sending identical content with the same key
    /// produces the same message id and is deduplicated. Defaults to a timestamp.
    /// Defaults to unset so existing foreign callers that construct
    /// `FfiSendMessageOpts` without this field still compile.
    #[uniffi(default = None)]
    pub idempotency_key: Option<String>,
}

impl From<FfiSendMessageOpts> for xmtp_mls::groups::send_message_opts::SendMessageOpts {
    fn from(opts: FfiSendMessageOpts) -> Self {
        xmtp_mls::groups::send_message_opts::SendMessageOpts {
            should_push: opts.should_push,
            idempotency_key: opts.idempotency_key,
        }
    }
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiGroupPermissionsOptions {
    Default,
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
    pub update_message_disappearing_policy: FfiPermissionPolicy,
    pub update_app_data_policy: FfiPermissionPolicy,
}

impl From<PreconfiguredPolicies> for FfiGroupPermissionsOptions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::Default => FfiGroupPermissionsOptions::Default,
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

        // MessageDisappearFromNS follows the same policy as MessageDisappearInNS
        metadata_permissions_map.insert(
            MetadataField::MessageDisappearFromNS.to_string(),
            policy_set
                .update_message_disappearing_policy
                .clone()
                .try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::MessageDisappearInNS.to_string(),
            policy_set.update_message_disappearing_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::AppData.to_string(),
            policy_set.update_app_data_policy.try_into()?,
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

/// Options for [`FfiConversation::update_app_data`]. A record (rather
/// than a bare `String` parameter) so future fields can be added
/// without breaking compiled apps.
///
/// WARNING: uniffi Records get NO default field values unless the field
/// carries `#[uniffi(default = ...)]`. Any field added later MUST carry
/// a uniffi default (and a serde/napi default on the wasm/node
/// `UpdateAppDataOptions`), or the generated Swift/Kotlin constructors
/// change and the addition breaks compiled apps.
#[derive(uniffi::Record, Clone, Default, Debug)]
pub struct FfiUpdateAppDataOptions {
    /// The new value for the group's opaque `APP_DATA` string slot.
    pub value: String,
    /// Optional compare-and-swap guard. When set, the update is abandoned
    /// with an `AppDataSuperseded` error — rather than overwriting — if the
    /// committed value is no longer this, including when another member's
    /// commit wins the race after this update was published. Leave unset for
    /// the historical last-writer-wins behavior.
    #[uniffi(default = None)]
    pub expected_value: Option<String>,
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiMetadataField {
    GroupName,
    Description,
    ImageUrlSquare,
    AppData,
    // Present on the wasm and node bindings from the start; added here
    // so the three bindings expose the same field set.
    MessageExpirationFromNs,
    MessageExpirationInNs,
}

impl From<&FfiMetadataField> for MetadataField {
    fn from(field: &FfiMetadataField) -> Self {
        match field {
            FfiMetadataField::GroupName => MetadataField::GroupName,
            FfiMetadataField::Description => MetadataField::Description,
            FfiMetadataField::ImageUrlSquare => MetadataField::GroupImageUrlSquare,
            FfiMetadataField::AppData => MetadataField::AppData,
            FfiMetadataField::MessageExpirationFromNs => MetadataField::MessageDisappearFromNS,
            FfiMetadataField::MessageExpirationInNs => MetadataField::MessageDisappearInNS,
        }
    }
}

/// Settings for disappearing messages in a conversation.
///
/// # Fields
///
/// * `from_ns` - The timestamp (in nanoseconds) from when messages should be tracked for deletion.
/// * `in_ns` - The duration (in nanoseconds) after which tracked messages will be deleted.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiMessageDisappearingSettings {
    pub from_ns: i64,
    pub in_ns: i64,
}

impl FfiMessageDisappearingSettings {
    pub(crate) fn new(from_ns: i64, in_ns: i64) -> Self {
        Self { from_ns, in_ns }
    }
}

impl From<MessageDisappearingSettings> for FfiMessageDisappearingSettings {
    fn from(value: MessageDisappearingSettings) -> Self {
        FfiMessageDisappearingSettings::new(value.from_ns, value.in_ns)
    }
}

#[derive(uniffi::Record, Debug, Clone, Copy)]
pub struct FfiCursor {
    sequence_id: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiConversationDebugInfo {
    pub epoch: u64,
    pub maybe_forked: bool,
    pub fork_details: String,
    pub is_commit_log_forked: Option<bool>,
    pub local_commit_log: String,
    pub remote_commit_log: String,
    pub cursor: Vec<FfiCursor>,
}

impl From<Cursor> for FfiCursor {
    fn from(value: Cursor) -> Self {
        FfiCursor {
            sequence_id: value.0,
        }
    }
}

impl FfiConversationDebugInfo {
    fn new(
        epoch: u64,
        maybe_forked: bool,
        fork_details: String,
        is_commit_log_forked: Option<bool>,
        local_commit_log: String,
        remote_commit_log: String,
        cursor: Vec<Cursor>,
    ) -> Self {
        Self {
            epoch,
            maybe_forked,
            fork_details,
            is_commit_log_forked,
            local_commit_log,
            remote_commit_log,
            cursor: cursor.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ConversationDebugInfo> for FfiConversationDebugInfo {
    fn from(value: ConversationDebugInfo) -> Self {
        FfiConversationDebugInfo::new(
            value.epoch,
            value.maybe_forked,
            value.fork_details,
            value.is_commit_log_forked,
            value.local_commit_log,
            value.remote_commit_log,
            value.cursor,
        )
    }
}

/// An MLS extension type advertised by an installation's key package or
/// present in a group's context. Mirrors
/// [`xmtp_mls::groups::MlsExtensionType`].
#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum FfiMlsExtensionType {
    ApplicationId,
    RatchetTree,
    RequiredCapabilities,
    ExternalPub,
    ExternalSenders,
    LastResort,
    ImmutableMetadata,
    AppDataDictionary,
    Unknown { id: u16 },
    Grease { id: u16 },
}

impl From<MlsExtensionType> for FfiMlsExtensionType {
    fn from(value: MlsExtensionType) -> Self {
        match value {
            MlsExtensionType::ApplicationId => FfiMlsExtensionType::ApplicationId,
            MlsExtensionType::RatchetTree => FfiMlsExtensionType::RatchetTree,
            MlsExtensionType::RequiredCapabilities => FfiMlsExtensionType::RequiredCapabilities,
            MlsExtensionType::ExternalPub => FfiMlsExtensionType::ExternalPub,
            MlsExtensionType::ExternalSenders => FfiMlsExtensionType::ExternalSenders,
            MlsExtensionType::LastResort => FfiMlsExtensionType::LastResort,
            MlsExtensionType::ImmutableMetadata => FfiMlsExtensionType::ImmutableMetadata,
            MlsExtensionType::AppDataDictionary => FfiMlsExtensionType::AppDataDictionary,
            MlsExtensionType::Unknown(id) => FfiMlsExtensionType::Unknown { id },
            MlsExtensionType::Grease(id) => FfiMlsExtensionType::Grease { id },
        }
    }
}

/// Capabilities for a single installation (device) in a group. Mirrors
/// [`xmtp_mls::groups::InstallationCapabilities`].
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiInstallationCapabilities {
    pub installation_id: Vec<u8>,
    pub is_own: bool,
    pub supported_extensions: Vec<FfiMlsExtensionType>,
    pub capabilities_known: bool,
}

/// Per-inbox installation capabilities. Mirrors
/// [`xmtp_mls::groups::InboxCapabilities`].
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiInboxCapabilities {
    pub inbox_id: String,
    pub installations: Vec<FfiInstallationCapabilities>,
}

/// A generic membership/capability snapshot for a group. Mirrors
/// [`xmtp_mls::groups::GroupMembershipCapabilities`]. It lists the
/// group context extensions and each installation's supported extensions.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiGroupMembershipCapabilities {
    pub context_extensions: Vec<FfiMlsExtensionType>,
    pub members: Vec<FfiInboxCapabilities>,
}

impl From<InstallationCapabilities> for FfiInstallationCapabilities {
    fn from(value: InstallationCapabilities) -> Self {
        FfiInstallationCapabilities {
            installation_id: value.installation_id,
            is_own: value.is_own,
            supported_extensions: value
                .supported_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            capabilities_known: value.capabilities_known,
        }
    }
}

impl From<InboxCapabilities> for FfiInboxCapabilities {
    fn from(value: InboxCapabilities) -> Self {
        FfiInboxCapabilities {
            inbox_id: value.inbox_id,
            installations: value.installations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<GroupMembershipCapabilities> for FfiGroupMembershipCapabilities {
    fn from(value: GroupMembershipCapabilities) -> Self {
        FfiGroupMembershipCapabilities {
            context_extensions: value
                .context_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            members: value.members.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RustMlsGroup> for FfiConversation {
    fn from(mls_group: RustMlsGroup) -> FfiConversation {
        FfiConversation { inner: mls_group }
    }
}

impl From<StoredConsentRecord> for FfiConsent {
    fn from(consent: StoredConsentRecord) -> Self {
        FfiConsent {
            entity: consent.entity,
            entity_type: match consent.entity_type {
                ConsentType::ConversationId => FfiConsentEntityType::ConversationId,
                ConsentType::InboxId => FfiConsentEntityType::InboxId,
            },
            state: consent.state.into(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiConversationMember {
    pub inbox_id: String,
    pub account_identifiers: Vec<FfiIdentifier>,
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

#[derive(uniffi::Enum, PartialEq, Debug)]
pub enum FfiGroupMembershipState {
    Allowed,
    Rejected,
    Pending,
    Restored,
    PendingRemove,
}

impl From<GroupMembershipState> for FfiGroupMembershipState {
    fn from(state: GroupMembershipState) -> Self {
        match state {
            GroupMembershipState::Allowed => FfiGroupMembershipState::Allowed,
            GroupMembershipState::Rejected => FfiGroupMembershipState::Rejected,
            GroupMembershipState::Pending => FfiGroupMembershipState::Pending,
            GroupMembershipState::Restored => FfiGroupMembershipState::Restored,
            GroupMembershipState::PendingRemove => FfiGroupMembershipState::PendingRemove,
        }
    }
}

impl From<FfiGroupMembershipState> for GroupMembershipState {
    fn from(state: FfiGroupMembershipState) -> Self {
        match state {
            FfiGroupMembershipState::Allowed => GroupMembershipState::Allowed,
            FfiGroupMembershipState::Rejected => GroupMembershipState::Rejected,
            FfiGroupMembershipState::Pending => GroupMembershipState::Pending,
            FfiGroupMembershipState::Restored => GroupMembershipState::Restored,
            FfiGroupMembershipState::PendingRemove => GroupMembershipState::PendingRemove,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiConsentEntityType {
    ConversationId,
    InboxId,
}

impl From<FfiConsentEntityType> for ConsentType {
    fn from(entity_type: FfiConsentEntityType) -> Self {
        match entity_type {
            FfiConsentEntityType::ConversationId => ConsentType::ConversationId,
            FfiConsentEntityType::InboxId => ConsentType::InboxId,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiDirection {
    Ascending,
    Descending,
}

impl From<FfiDirection> for SortDirection {
    fn from(direction: FfiDirection) -> Self {
        match direction {
            FfiDirection::Ascending => SortDirection::Ascending,
            FfiDirection::Descending => SortDirection::Descending,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiSortBy {
    SentAt,
    InsertedAt,
}

impl From<FfiSortBy> for SortBy {
    fn from(sort_by: FfiSortBy) -> Self {
        match sort_by {
            FfiSortBy::SentAt => SortBy::SentAt,
            FfiSortBy::InsertedAt => SortBy::InsertedAt,
        }
    }
}

impl From<FfiMessageDisappearingSettings> for MessageDisappearingSettings {
    fn from(settings: FfiMessageDisappearingSettings) -> Self {
        MessageDisappearingSettings::new(settings.from_ns, settings.in_ns)
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
    pub direction: Option<FfiDirection>,
    pub content_types: Option<Vec<FfiContentType>>,
    pub exclude_content_types: Option<Vec<FfiContentType>>,
    pub exclude_sender_inbox_ids: Option<Vec<String>>,
    pub sort_by: Option<FfiSortBy>,
    pub inserted_after_ns: Option<i64>,
    pub inserted_before_ns: Option<i64>,
}

impl From<FfiListMessagesOptions> for MsgQueryArgs {
    fn from(opts: FfiListMessagesOptions) -> Self {
        MsgQueryArgs {
            kind: None,
            sent_before_ns: opts.sent_before_ns,
            sent_after_ns: opts.sent_after_ns,
            limit: opts.limit,
            delivery_status: opts.delivery_status.map(Into::into),
            direction: opts.direction.map(Into::into),
            content_types: opts
                .content_types
                .map(|types| types.into_iter().map(Into::into).collect()),
            exclude_content_types: opts
                .exclude_content_types
                .map(|types| types.into_iter().map(Into::into).collect()),
            exclude_sender_inbox_ids: opts.exclude_sender_inbox_ids,
            sort_by: opts.sort_by.map(Into::into),
            inserted_after_ns: opts.inserted_after_ns,
            inserted_before_ns: opts.inserted_before_ns,
            exclude_disappearing: false,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiContentType {
    Unknown,
    Text,
    GroupMembershipChange,
    GroupUpdated,
    Reaction,
    ReadReceipt,
    Reply,
    Attachment,
    RemoteAttachment,
    TransactionReference,
    WalletSendCalls,
    LeaveRequest,
    Markdown,
    Actions,
    Intent,
    MultiRemoteAttachment,
}

impl From<FfiContentType> for ContentType {
    fn from(value: FfiContentType) -> Self {
        match value {
            FfiContentType::Unknown => ContentType::Unknown,
            FfiContentType::Text => ContentType::Text,
            FfiContentType::GroupMembershipChange => ContentType::GroupMembershipChange,
            FfiContentType::GroupUpdated => ContentType::GroupUpdated,
            FfiContentType::Reaction => ContentType::Reaction,
            FfiContentType::ReadReceipt => ContentType::ReadReceipt,
            FfiContentType::Reply => ContentType::Reply,
            FfiContentType::Attachment => ContentType::Attachment,
            FfiContentType::RemoteAttachment => ContentType::RemoteAttachment,
            FfiContentType::TransactionReference => ContentType::TransactionReference,
            FfiContentType::WalletSendCalls => ContentType::WalletSendCalls,
            FfiContentType::LeaveRequest => ContentType::LeaveRequest,
            FfiContentType::Markdown => ContentType::Markdown,
            FfiContentType::Actions => ContentType::Actions,
            FfiContentType::Intent => ContentType::Intent,
            FfiContentType::MultiRemoteAttachment => ContentType::MultiRemoteAttachment,
        }
    }
}

#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiConversationMessageKind {
    Application,
    MembershipChange,
}

impl From<GroupMessageKind> for FfiConversationMessageKind {
    fn from(kind: GroupMessageKind) -> Self {
        match kind {
            GroupMessageKind::Application => FfiConversationMessageKind::Application,
            GroupMessageKind::MembershipChange => FfiConversationMessageKind::MembershipChange,
        }
    }
}

#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiConversationType {
    Group,
    Dm,
    Sync,
    Oneshot,
}

impl From<ConversationType> for FfiConversationType {
    fn from(kind: ConversationType) -> Self {
        match kind {
            ConversationType::Group => FfiConversationType::Group,
            ConversationType::Dm => FfiConversationType::Dm,
            ConversationType::Sync => FfiConversationType::Sync,
            ConversationType::Oneshot => FfiConversationType::Oneshot,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub conversation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content: Vec<u8>,
    pub kind: FfiConversationMessageKind,
    pub delivery_status: FfiDeliveryStatus,
    pub sequence_id: u64,
    pub inserted_at_ns: i64,
    pub expire_at_ns: Option<i64>,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            conversation_id: msg.group_id.into(),
            sender_inbox_id: msg.sender_inbox_id,
            content: msg.decrypted_message_bytes,
            kind: msg.kind.into(),
            delivery_status: msg.delivery_status.into(),
            sequence_id: msg.sequence_id as u64,
            inserted_at_ns: msg.inserted_at_ns,
            expire_at_ns: msg.expire_at_ns,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct FfiApiStats {
    pub publish: u64,
    pub query: u64,
    pub query_newest: u64,
    pub subscribe: u64,
    pub subscribe_static: u64,
}

impl From<ApiStats> for FfiApiStats {
    fn from(stats: ApiStats) -> Self {
        Self {
            publish: stats.publish.get_count() as u64,
            query: stats.query.get_count() as u64,
            query_newest: stats.query_newest.get_count() as u64,
            subscribe: stats.subscribe.get_count() as u64,
            subscribe_static: stats.subscribe_static.get_count() as u64,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct FfiIdentityStats {
    pub get_inbox_ids: u64,
    pub verify_smart_contract_wallet_signatures: u64,
}

impl From<IdentityStats> for FfiIdentityStats {
    fn from(stats: IdentityStats) -> Self {
        Self {
            get_inbox_ids: stats.get_inbox_ids.get_count() as u64,
            verify_smart_contract_wallet_signatures: stats
                .verify_smart_contract_wallet_signatures
                .get_count() as u64,
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
            consented_at_ns: now_ns(),
        }
    }
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiPreferenceUpdate {
    HMAC { key: Vec<u8> },
}

#[derive(uniffi::Object)]
pub struct FfiConversationMetadata {
    pub(crate) inner: Arc<GroupMetadata>,
}

#[uniffi::export]
impl FfiConversationMetadata {
    pub fn creator_inbox_id(&self) -> String {
        self.inner.creator_inbox_id.clone()
    }

    pub fn conversation_type(&self) -> FfiConversationType {
        match self.inner.conversation_type {
            ConversationType::Group => FfiConversationType::Group,
            ConversationType::Dm => FfiConversationType::Dm,
            ConversationType::Sync => FfiConversationType::Sync,
            ConversationType::Oneshot => FfiConversationType::Oneshot,
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroupPermissions {
    pub(crate) inner: Arc<GroupMutablePermissions>,
}

#[uniffi::export]
impl FfiGroupPermissions {
    #[xmtp_common::err_span]
    pub fn policy_type(&self) -> Result<FfiGroupPermissionsOptions, FfiError> {
        if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
            Ok(preconfigured_policy.into())
        } else {
            Ok(FfiGroupPermissionsOptions::CustomPolicy)
        }
    }

    #[xmtp_common::err_span]
    pub fn policy_set(&self) -> Result<FfiPermissionPolicySet, FfiError> {
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
            update_message_disappearing_policy: get_policy(
                MetadataField::MessageDisappearInNS.as_str(),
            ),
            update_app_data_policy: get_policy(MetadataField::AppData.as_str()),
        })
    }
}
