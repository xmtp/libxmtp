use super::{
    group_membership::GroupMembership,
    group_permissions::{MembershipPolicies, MetadataPolicies, PermissionsPolicies},
};
use crate::groups::mls_ext::WelcomePointersExtension;
use openmls::key_packages::KeyPackage;
use openmls::prelude::{
    MlsMessageOut,
    tls_codec::{Error as TlsCodecError, Serialize},
};
use prost::{Message, bytes::Bytes};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use xmtp_common::types::Address;
use xmtp_configuration::GROUP_KEY_ROTATION_INTERVAL_NS;
use xmtp_cryptography::secret::{Zeroize, Zeroizing};
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2, WrapperAlgorithm};
use xmtp_mls_group_state::{
    app_data::{
        component_id::ComponentId,
        fields::{FieldWrite, WriteOperation},
    },
    group_mutable_metadata::MetadataField,
};
use xmtp_proto::{
    ConversionError,
    xmtp::mls::database::{
        AccountAddresses, AddressesOrInstallationIds as AddressesOrInstallationIdsProtoWrapper,
        CommitPendingProposalsData, InstallationIds, PostCommitAction as PostCommitActionProto,
        ProposeGroupContextExtensionData, ProposeMemberUpdateData, ReaddInstallationsData,
        SendMessageData, UpdateAdminListsData, UpdateGroupMembershipData, UpdateMetadataData,
        UpdatePermissionData,
        addresses_or_installation_ids::AddressesOrInstallationIds as AddressesOrInstallationIdsProto,
        commit_pending_proposals_data,
        post_commit_action::{
            Installation as InstallationProto, Kind as PostCommitActionKind,
            SendWelcomes as SendWelcomesProto,
        },
        propose_group_context_extension_data, propose_member_update_data,
        readd_installations_data::{
            V1 as ReaddInstallationsV1, Version as ReaddInstallationsVersion,
        },
        send_message_data::{V1 as SendMessageV1, Version as SendMessageVersion},
        update_admin_lists_data::{V1 as UpdateAdminListsV1, Version as UpdateAdminListsVersion},
        update_group_membership_data::{
            V1 as UpdateGroupMembershipV1, Version as UpdateGroupMembershipVersion,
        },
        update_metadata_data::{V1 as UpdateMetadataV1, Version as UpdateMetadataVersion},
        update_permission_data::{
            self, V1 as UpdatePermissionV1, Version as UpdatePermissionVersion,
        },
    },
    xmtp::mls::message_contents::{ComponentType, WelcomePointerWrapperAlgorithm},
};

mod queue;
pub use queue::*;

mod app_data;
mod membership;
mod permissions;
mod post_commit;
mod proposals;

pub use app_data::AppDataUpdateIntentData;
pub use membership::UpdateGroupMembershipResult;
pub(crate) use membership::{ReaddInstallationsIntentData, UpdateGroupMembershipIntentData};
pub use permissions::{
    AdminListActionType, PermissionPolicyOption, PermissionUpdateType, UpdateAdminListIntentData,
    UpdatePermissionIntentData,
};
pub use post_commit::{
    Installation, MembershipDiffWithKeyPackages, PostCommitAction, SendWelcomesAction,
};
#[cfg(test)]
pub(crate) use proposals::ProposeGroupContextExtensionsIntentData;
pub(crate) use proposals::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData};

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("conversion error: {0}")]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error("missing update permission")]
    MissingUpdatePermissionVersion,
    #[error("missing payload")]
    MissingPayload,
    #[error("missing update admin version")]
    MissingUpdateAdminVersion,
    #[error("missing post commit action")]
    MissingPostCommit,
    #[error("unsupported permission version")]
    UnsupportedPermissionVersion,
    #[error("unknown permission update type")]
    UnknownPermissionUpdateType,
    #[error("unknown value for PermissionPolicyOption")]
    UnknownPermissionPolicyOption,
    #[error("unknown value for AdminListActionType")]
    UnknownAdminListAction,
    #[error("intent payload error: {0}")]
    Generic(String),
}

impl From<prost::DecodeError> for IntentError {
    fn from(error: prost::DecodeError) -> Self {
        IntentError::Conversion(xmtp_proto::ConversionError::Decode(error))
    }
}

/// The encoded envelope of a message to send. It can hold a device-sync
/// HMAC root key, so it is wiped on drop and `Debug` shows only its length.
#[derive(Debug, Clone)]
pub struct SendMessageIntentData {
    pub message: xmtp_cryptography::Secret,
}

impl SendMessageIntentData {
    pub fn new(message: Vec<u8>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The returned bytes are the stored intent. The proto temporary is wiped
    /// after encoding.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut data = SendMessageData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.as_slice().to_vec(),
            })),
        };
        let bytes = data.encode_to_vec();
        if let Some(SendMessageVersion::V1(v1)) = &mut data.version {
            v1.payload_bytes.zeroize();
        }
        bytes
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessageData::decode(data)?;
        let payload_bytes = match msg.version {
            Some(SendMessageVersion::V1(mut v1)) => std::mem::take(&mut v1.payload_bytes),
            None => return Err(IntentError::MissingPayload),
        };

        Ok(Self::new(payload_bytes))
    }
}

impl From<SendMessageIntentData> for Vec<u8> {
    fn from(intent: SendMessageIntentData) -> Self {
        intent.to_bytes()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AddressesOrInstallationIds {
    AccountAddresses(Vec<String>),
    InstallationIds(Vec<Vec<u8>>),
}

impl From<AddressesOrInstallationIds> for AddressesOrInstallationIdsProtoWrapper {
    fn from(address_or_id: AddressesOrInstallationIds) -> Self {
        match address_or_id {
            AddressesOrInstallationIds::AccountAddresses(account_addresses) => {
                AddressesOrInstallationIdsProtoWrapper {
                    addresses_or_installation_ids: Some(
                        AddressesOrInstallationIdsProto::AccountAddresses(AccountAddresses {
                            account_addresses,
                        }),
                    ),
                }
            }
            AddressesOrInstallationIds::InstallationIds(installation_ids) => {
                AddressesOrInstallationIdsProtoWrapper {
                    addresses_or_installation_ids: Some(
                        AddressesOrInstallationIdsProto::InstallationIds(InstallationIds {
                            installation_ids,
                        }),
                    ),
                }
            }
        }
    }
}

impl TryFrom<AddressesOrInstallationIdsProtoWrapper> for AddressesOrInstallationIds {
    type Error = IntentError;

    fn try_from(wrapper: AddressesOrInstallationIdsProtoWrapper) -> Result<Self, Self::Error> {
        match wrapper.addresses_or_installation_ids {
            Some(AddressesOrInstallationIdsProto::AccountAddresses(addrs)) => Ok(
                AddressesOrInstallationIds::AccountAddresses(addrs.account_addresses),
            ),
            Some(AddressesOrInstallationIdsProto::InstallationIds(ids)) => Ok(
                AddressesOrInstallationIds::InstallationIds(ids.installation_ids),
            ),
            _ => Err(IntentError::MissingPayload),
        }
    }
}

impl From<Vec<Address>> for AddressesOrInstallationIds {
    fn from(addrs: Vec<Address>) -> Self {
        AddressesOrInstallationIds::AccountAddresses(addrs)
    }
}

impl From<Vec<Vec<u8>>> for AddressesOrInstallationIds {
    fn from(installation_ids: Vec<Vec<u8>>) -> Self {
        AddressesOrInstallationIds::InstallationIds(installation_ids)
    }
}

/// A metadata update. Its values can hold the commit-log signer's private key
/// as hex, so they are wiped on drop and `Debug` shows only the field name.
#[derive(Clone)]
pub struct UpdateMetadataIntentData {
    pub field_name: String,
    pub field_value: String,
    /// Compare-and-swap guard. When set, publishing abandons this intent
    /// unless the field's currently committed value still equals this.
    /// `None` keeps the historical last-writer-wins behavior.
    pub expected_field_value: Option<String>,
}

impl std::fmt::Debug for UpdateMetadataIntentData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateMetadataIntentData")
            .field("field_name", &self.field_name)
            .field("field_value", &"<redacted>")
            .field(
                "expected_field_value",
                &self.expected_field_value.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Drop for UpdateMetadataIntentData {
    fn drop(&mut self) {
        self.field_value.zeroize();
        self.expected_field_value.zeroize();
    }
}

impl UpdateMetadataIntentData {
    pub fn new(field_name: String, field_value: String) -> Self {
        Self {
            field_name,
            field_value,
            expected_field_value: None,
        }
    }

    /// A guarded update: publishing abandons the intent (marking it
    /// [`xmtp_db::group_intent::IntentState::Superseded`]) unless the
    /// committed value still equals `expected_field_value`.
    pub fn new_guarded(
        field_name: String,
        field_value: String,
        expected_field_value: String,
    ) -> Self {
        Self {
            field_name,
            field_value,
            expected_field_value: Some(expected_field_value),
        }
    }

    pub fn new_update_group_name(group_name: String) -> Self {
        Self::new(MetadataField::GroupName.to_string(), group_name)
    }

    pub fn new_update_group_image_url_square(group_image_url_square: String) -> Self {
        Self::new(
            MetadataField::GroupImageUrlSquare.to_string(),
            group_image_url_square,
        )
    }

    pub fn new_update_group_description(group_description: String) -> Self {
        Self::new(MetadataField::Description.to_string(), group_description)
    }

    pub fn new_update_app_data(app_data: String, expected_app_data: Option<String>) -> Self {
        match expected_app_data {
            Some(expected) => {
                Self::new_guarded(MetadataField::AppData.to_string(), app_data, expected)
            }
            None => Self::new(MetadataField::AppData.to_string(), app_data),
        }
    }

    pub fn new_update_conversation_message_disappear_from_ns(from_ns: i64) -> Self {
        Self::new(
            MetadataField::MessageDisappearFromNS.to_string(),
            from_ns.to_string(),
        )
    }
    pub fn new_update_conversation_message_disappear_in_ns(in_ns: i64) -> Self {
        Self::new(
            MetadataField::MessageDisappearInNS.to_string(),
            in_ns.to_string(),
        )
    }

    pub fn new_update_group_min_version_to_match_self(min_version: String) -> Self {
        Self::new(
            MetadataField::MinimumSupportedProtocolVersion.to_string(),
            min_version,
        )
    }

    pub fn new_update_commit_log_signer(commit_log_signer: xmtp_cryptography::Secret) -> Self {
        Self::new(
            MetadataField::CommitLogSigner.to_string(),
            hex::encode(commit_log_signer.as_slice()),
        )
    }
}

/// The returned bytes are the stored intent. The values move into a proto
/// temporary that is wiped after encoding.
impl From<UpdateMetadataIntentData> for Vec<u8> {
    fn from(mut intent: UpdateMetadataIntentData) -> Self {
        let data = UpdateMetadataData {
            version: Some(UpdateMetadataVersion::V1(UpdateMetadataV1 {
                field_name: std::mem::take(&mut intent.field_name),
                field_value: std::mem::take(&mut intent.field_value),
                expected_field_value: intent.expected_field_value.take(),
            })),
        };
        // Sized up front, so encoding never frees a partial copy. The proto
        // wipes its values when it drops.
        data.encode_to_vec()
    }
}

impl TryFrom<Vec<u8>> for UpdateMetadataIntentData {
    type Error = IntentError;

    /// Decodes a stored intent. The input is wiped, and the values move out of
    /// the decoded proto.
    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let data = Zeroizing::new(data);
        let msg = UpdateMetadataData::decode(data.as_slice())?;
        let Some(UpdateMetadataVersion::V1(mut v1)) = msg.version else {
            return Err(IntentError::MissingPayload);
        };
        // Absent on intents queued before the guard existed, and on payloads
        // written by a client that predates it — both mean last-writer-wins.
        Ok(Self {
            field_name: std::mem::take(&mut v1.field_name),
            field_value: std::mem::take(&mut v1.field_value),
            expected_field_value: v1.expected_field_value.take(),
        })
    }
}

#[cfg(test)]
mod app_data_update_intent_tests {
    use super::*;

    #[test]
    fn round_trip_basic() {
        let intent = AppDataUpdateIntentData::new(0x800C, vec![0xAA, 0xBB, 0xCC]);
        let bytes: Vec<u8> = intent.clone().into();
        let restored = AppDataUpdateIntentData::try_from(bytes).unwrap();
        assert_eq!(restored, intent);
    }

    #[test]
    fn empty_payload_round_trips() {
        // Empty payload bytes (a "no-op" intent shape) must round-trip
        // cleanly — useful for components that legitimately write
        // empty values (e.g., revoke clears EXTERNAL_COMMIT_POLICY's
        // symmetric_key to zero-length).
        let intent = AppDataUpdateIntentData::new(0x800C, vec![]);
        let bytes: Vec<u8> = intent.clone().into();
        let restored = AppDataUpdateIntentData::try_from(bytes).unwrap();
        assert_eq!(restored, intent);
    }

    #[test]
    fn missing_version_variant_surfaces_error() {
        // Proto with no `version` oneof set decodes to `version: None`
        // on unknown-variant tolerance. Reject explicitly — an intent
        // payload without a recognised version variant is malformed.
        use prost::Message;
        use xmtp_proto::xmtp::mls::database::AppDataUpdateData;
        let bytes = AppDataUpdateData { version: None }.encode_to_vec();
        let err = AppDataUpdateIntentData::try_from(bytes).unwrap_err();
        match err {
            IntentError::Generic(msg) => assert!(msg.contains("missing version")),
            _ => panic!("expected Generic error, got {err:?}"),
        }
    }

    #[test]
    fn component_id_overflow_surfaces_error() {
        // u16 component_id is widened to u32 on the wire. A payload that
        // somehow carries a value above u16::MAX (e.g., a malformed
        // intent or one written by a hypothetical future client widening
        // the id space) must be rejected — the dispatcher narrows to
        // u16 and we'd silently lose information without the explicit
        // check.
        use prost::Message;
        use xmtp_proto::xmtp::mls::database::{
            AppDataUpdateData, app_data_update_data::V1 as AppDataUpdateDataV1,
            app_data_update_data::Version as AppDataUpdateVersion,
        };
        let bytes = AppDataUpdateData {
            version: Some(AppDataUpdateVersion::V1(AppDataUpdateDataV1 {
                component_id: u16::MAX as u32 + 1,
                payload: vec![],
            })),
        }
        .encode_to_vec();
        let err = AppDataUpdateIntentData::try_from(bytes).unwrap_err();
        match err {
            IntentError::Generic(msg) => assert!(msg.contains("exceeds u16")),
            _ => panic!("expected Generic error, got {err:?}"),
        }
    }

    /// A queued field write must reach the publisher exactly as encoded:
    /// its ID, the type it was encoded under (the publisher's stale-type
    /// check compares against it), and each operation kind.
    #[xmtp_common::test]
    fn field_writes_round_trip() {
        let write = |id, component_type, operation| FieldWrite {
            component_id: ComponentId::new(id),
            component_type,
            operation,
        };
        let intent = AppDataUpdateIntentData::Fields(vec![
            write(
                0x8001,
                ComponentType::String,
                WriteOperation::Update(b"a".to_vec()),
            ),
            write(0x8002, ComponentType::Bytes, WriteOperation::Remove),
            write(
                0x8003,
                ComponentType::TlsMapInboxIdString,
                WriteOperation::SetOwn(b"b".to_vec()),
            ),
            write(
                0x8004,
                ComponentType::TlsMapInboxIdBytes,
                WriteOperation::ClearOwn,
            ),
        ]);
        let bytes: Vec<u8> = intent.clone().into();
        assert_eq!(AppDataUpdateIntentData::try_from(bytes).unwrap(), intent);
    }

    /// A field write whose encoding type or operation this build cannot
    /// read fails closed rather than publishing a guess.
    #[xmtp_common::test]
    fn unreadable_field_writes_fail_closed() {
        use xmtp_proto::xmtp::mls::database::{
            AppDataUpdateData,
            app_data_update_data::{
                FieldWrite as FieldWriteProto, V2, Version, field_write::Operation,
            },
        };
        let decode = |component_type, operation| {
            AppDataUpdateIntentData::try_from(
                AppDataUpdateData {
                    version: Some(Version::V2(V2 {
                        writes: vec![FieldWriteProto {
                            component_id: 0x8001,
                            component_type,
                            operation,
                        }],
                    })),
                }
                .encode_to_vec(),
            )
        };
        assert!(decode(ComponentType::Bytes.into(), Some(Operation::Remove(true))).is_ok());
        assert!(decode(99, Some(Operation::Remove(true))).is_err());
        assert!(decode(ComponentType::Bytes.into(), None).is_err());
    }

    #[test]
    fn malformed_proto_bytes_surface_decode_error() {
        // Random bytes that aren't a valid proto must surface a
        // prost::DecodeError via the IntentError::Conversion path.
        let bytes = vec![0xFF; 16];
        assert!(AppDataUpdateIntentData::try_from(bytes).is_err());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::groups::test_support::{EnginePeer, EngineTestGroup, EngineTestPeer};
    use openmls::prelude::{ProcessedMessageContent, ProtocolMessage};
    use xmtp_mls_core::context::CoreContext;

    use xmtp_db::XmtpOpenMlsProviderRef;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message.as_slice(), message.as_slice());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_serialize_update_membership() {
        let mut membership_updates = HashMap::new();
        membership_updates.insert("foo".to_string(), 123);

        let intent = UpdateGroupMembershipIntentData::new(
            membership_updates,
            vec!["bar".to_string()],
            vec![vec![1, 2, 3]],
        );

        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateGroupMembershipIntentData = as_bytes.try_into().unwrap();

        assert_eq!(
            intent.membership_updates,
            restored_intent.membership_updates
        );

        assert_eq!(intent.removed_members, restored_intent.removed_members);

        assert_eq!(
            intent.failed_installations,
            restored_intent.failed_installations
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn membership_refresh_never_lowers_committed_sequence() {
        let mut committed = GroupMembership::new();
        committed.add("stale".to_string(), 5);
        committed.add("raised".to_string(), 3);
        committed.add("removed".to_string(), 2);
        let intent = UpdateGroupMembershipIntentData::new(
            HashMap::from([
                ("stale".to_string(), 4),
                ("raised".to_string(), 7),
                ("added".to_string(), 1),
                ("removed".to_string(), 9),
            ]),
            vec!["removed".to_string()],
            vec![],
        );
        let updated = intent.apply_to_group_membership(&committed);
        assert_eq!(updated.get("stale"), Some(&5));
        assert_eq!(updated.get("raised"), Some(&7));
        assert_eq!(updated.get("added"), Some(&1));
        assert_eq!(updated.get("removed"), None);
        assert_eq!(updated.members.len(), 3);
    }

    #[xmtp_common::test]
    async fn test_serialize_update_metadata() {
        let intent = UpdateMetadataIntentData::new_update_group_name("group name".to_string());
        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateMetadataIntentData =
            UpdateMetadataIntentData::try_from(as_bytes).unwrap();

        assert_eq!(intent.field_value, restored_intent.field_value);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_serialize_readd_installations() {
        let readded_installations = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        let intent = ReaddInstallationsIntentData::new(readded_installations.clone());

        let as_bytes: Vec<u8> = intent.into();
        let restored_intent: ReaddInstallationsIntentData = as_bytes.try_into().unwrap();

        assert_eq!(readded_installations, restored_intent.readded_installations);
    }

    #[xmtp_common::test]
    async fn test_key_rotation_before_first_message() {
        let client_a = EngineTestPeer::new_with(|o| o.triggers = true).await;
        let client_b = EngineTestPeer::new_with(|o| o.triggers = true).await;

        // client A makes a group with client B, and then sends a message to client B.
        let group_a = client_a.create_group(None, None).expect("create group");
        group_a.add_members(&[client_b.inbox_id()]).await.unwrap();
        group_a
            .send_message(b"First message from A", SendMessageOpts::default())
            .await
            .unwrap();

        // No key rotation is needed because A's commit to add B rotates the key.
        // The Add proposal, membership AppDataUpdate proposal, commit, and message are published.
        verify_num_payloads_in_group(&group_a, 4).await;

        // Client B sends a message to Client A
        let groups_b = client_b.sync_welcomes().await.unwrap();
        assert_eq!(groups_b.len(), 1);
        let group_b = groups_b[0].clone();
        group_b
            .send_message(b"First message from B", SendMessageOpts::default())
            .await
            .expect("send message");

        // B must perform a key rotation before sending their first message.
        // The initial four envelopes are followed by B's key rotation and message.
        let payloads_a = verify_num_payloads_in_group(&group_a, 6).await;
        let payloads_b = verify_num_payloads_in_group(&group_b, 6).await;

        // Verify key rotation payload
        for i in 0..payloads_a.len() {
            assert_eq!(payloads_a[i].payload_hash, payloads_b[i].payload_hash);
        }
        verify_commit_updates_leaf_node(&group_a, &payloads_a[4]);

        // Client B sends another message to Client A, and Client A sends another message to Client B.
        group_b
            .send_message(b"Second message from B", SendMessageOpts::default())
            .await
            .expect("send message");
        group_a
            .send_message(b"Second message from A", SendMessageOpts::default())
            .await
            .expect("send message");

        // Group should only have two additional messages. No more key rotations are needed.
        verify_num_payloads_in_group(&group_a, 8).await;
        verify_num_payloads_in_group(&group_b, 8).await;
    }

    async fn verify_num_payloads_in_group(
        group: &EngineTestGroup,
        num_messages: usize,
    ) -> Vec<xmtp_proto::types::GroupMessage> {
        let messages = group
            .context
            .api()
            .query_group_messages(group.group_id)
            .await
            .unwrap();
        assert_eq!(messages.len(), num_messages);
        messages
    }

    fn verify_commit_updates_leaf_node(
        group: &EngineTestGroup,
        message: &xmtp_proto::types::GroupMessage,
    ) {
        let mls_message = message.message.clone();
        let mls_message = match mls_message {
            ProtocolMessage::PrivateMessage(mls_message) => mls_message,
            _ => panic!("error mls_message"),
        };

        let storage = group.context.mls_storage();
        let decrypted_message = group
            .load_mls_group_with_lock(storage, |mut mls_group| {
                Ok(mls_group
                    .process_message(&XmtpOpenMlsProviderRef::new(storage), mls_message.clone())
                    .unwrap())
            })
            .unwrap();

        let staged_commit = match decrypted_message.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => *staged_commit,
            _ => panic!("error staged_commit"),
        };

        // Check there is indeed some updated leaf node, which means the key update works.
        let path_update_leaf_node = staged_commit.update_path_leaf_node();
        assert!(path_update_leaf_node.is_some());
    }
}
