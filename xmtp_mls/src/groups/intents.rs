use openmls::prelude::{
    tls_codec::{Error as TlsCodecError, Serialize},
    MlsMessageOut,
};
use prost::{DecodeError, Message};
use thiserror::Error;

use xmtp_proto::xmtp::mls::database::{
    addresses_or_installation_ids::AddressesOrInstallationIds as AddressesOrInstallationIdsProto,
    post_commit_action::{
        Installation as InstallationProto, Kind as PostCommitActionKind,
        SendWelcomes as SendWelcomesProto,
    },
    AccountAddresses, AddressesOrInstallationIds as AddressesOrInstallationIdsProtoWrapper,
    InstallationIds, PostCommitAction as PostCommitActionProto,
};

use super::{scoped_client::ScopedGroupClient, GroupError, MlsGroup};
use crate::{
    configuration::GROUP_KEY_ROTATION_INTERVAL_NS,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    XmtpOpenMlsProvider,
};
use xmtp_common::types::Address;
use xmtp_db::{
    db_connection::DbConnection,
    group_intent::{IntentKind, NewGroupIntent, StoredGroupIntent},
    ProviderTransactions,
};

mod admin;
mod group_membership;
mod key_update;
mod metadata;
mod permission;
mod send_message;
pub use admin::*;
pub use group_membership::*;
pub use key_update::*;
pub use metadata::*;
pub use permission::*;
pub use send_message::*;

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
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
}

impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    pub fn queue_intent(
        &self,
        provider: &XmtpOpenMlsProvider,
        intent_kind: IntentKind,
        intent_data: Vec<u8>,
        should_push: bool,
    ) -> Result<StoredGroupIntent, GroupError> {
        let res = provider.transaction(|provider| {
            let conn = provider.conn_ref();
            self.queue_intent_with_conn(conn, intent_kind, intent_data, should_push)
        });

        res
    }

    fn queue_intent_with_conn(
        &self,
        conn: &DbConnection,
        intent_kind: IntentKind,
        intent_data: Vec<u8>,
        should_push: bool,
    ) -> Result<StoredGroupIntent, GroupError> {
        if intent_kind == IntentKind::SendMessage {
            self.maybe_insert_key_update_intent(conn)?;
        }

        let intent = conn.insert_group_intent(NewGroupIntent::new(
            intent_kind,
            self.group_id.clone(),
            intent_data,
            should_push,
        ))?;

        if intent_kind != IntentKind::SendMessage {
            conn.update_rotated_at_ns(self.group_id.clone())?;
        }
        tracing::debug!(inbox_id = self.client.inbox_id(), intent_kind = %intent_kind, "queued intent");

        Ok(intent)
    }

    fn maybe_insert_key_update_intent(&self, conn: &DbConnection) -> Result<(), GroupError> {
        let last_rotated_at_ns = conn.get_rotated_at_ns(self.group_id.clone())?;
        let now_ns = xmtp_common::time::now_ns();
        let elapsed_ns = now_ns - last_rotated_at_ns;
        if elapsed_ns > GROUP_KEY_ROTATION_INTERVAL_NS {
            self.queue_intent_with_conn(conn, IntentKind::KeyUpdate, vec![], false)?;
        }
        Ok(())
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

#[derive(Debug, Clone)]
pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

#[derive(Debug, Clone)]
pub struct Installation {
    pub(crate) installation_key: Vec<u8>,
    pub(crate) hpke_public_key: Vec<u8>,
}

impl Installation {
    pub fn from_verified_key_package(key_package: &VerifiedKeyPackageV2) -> Self {
        Self {
            installation_key: key_package.installation_id(),
            hpke_public_key: key_package.hpke_init_key(),
        }
    }
}

impl From<Installation> for InstallationProto {
    fn from(installation: Installation) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
        }
    }
}

impl From<InstallationProto> for Installation {
    fn from(installation: InstallationProto) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendWelcomesAction {
    pub installations: Vec<Installation>,
    pub welcome_message: Vec<u8>,
}

impl SendWelcomesAction {
    pub fn new(installations: Vec<Installation>, welcome_message: Vec<u8>) -> Self {
        Self {
            installations,
            welcome_message,
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        PostCommitActionProto {
            kind: Some(PostCommitActionKind::SendWelcomes(SendWelcomesProto {
                installations: self
                    .installations
                    .clone()
                    .into_iter()
                    .map(|i| i.into())
                    .collect(),
                welcome_message: self.welcome_message.clone(),
            })),
        }
        .encode_to_vec()
    }
}

impl PostCommitAction {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            PostCommitAction::SendWelcomes(action) => action.to_bytes(),
        }
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let decoded = PostCommitActionProto::decode(data)?;
        match decoded.kind {
            Some(PostCommitActionKind::SendWelcomes(proto)) => {
                Ok(Self::SendWelcomes(SendWelcomesAction::new(
                    proto.installations.into_iter().map(|i| i.into()).collect(),
                    proto.welcome_message,
                )))
            }
            None => Err(IntentError::MissingPostCommit),
        }
    }

    pub(crate) fn from_welcome(
        welcome: MlsMessageOut,
        installations: Vec<Installation>,
    ) -> Result<Self, IntentError> {
        let welcome_bytes = welcome.tls_serialize_detached()?;

        Ok(Self::SendWelcomes(SendWelcomesAction::new(
            installations,
            welcome_bytes,
        )))
    }
}

impl TryFrom<Vec<u8>> for PostCommitAction {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        PostCommitAction::from_bytes(data.as_slice())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use openmls::prelude::{MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent};
    use tls_codec::Deserialize;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::xmtp::mls::api::v1::{group_message, GroupMessage};

    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions, utils::test::FullXmtpClient,
    };

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message, message);
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_serialize_update_metadata() {
        let intent = UpdateMetadataIntentData::new_update_group_name("group name".to_string());
        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateMetadataIntentData =
            UpdateMetadataIntentData::try_from(as_bytes).unwrap();

        assert_eq!(intent.field_value, restored_intent.field_value);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_key_rotation_before_first_message() {
        let client_a = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_b = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // client A makes a group with client B, and then sends a message to client B.
        let group_a = client_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group_a
            .add_members_by_inbox_id(&[client_b.inbox_id()])
            .await
            .unwrap();
        group_a.send_message(b"First message from A").await.unwrap();

        // No key rotation needed, because A's commit to add B already performs a rotation.
        // Group should have a commit to add client B, followed by A's message.
        verify_num_payloads_in_group(&group_a, 2).await;

        // Client B sends a message to Client A
        let groups_b = client_b
            .sync_welcomes(&client_b.mls_provider().unwrap())
            .await
            .unwrap();
        assert_eq!(groups_b.len(), 1);
        let group_b = groups_b[0].clone();
        group_b
            .send_message(b"First message from B")
            .await
            .expect("send message");

        // B must perform a key rotation before sending their first message.
        // Group should have a commit to add B, A's message, B's key rotation and then B's message.
        let payloads_a = verify_num_payloads_in_group(&group_a, 4).await;
        let payloads_b = verify_num_payloads_in_group(&group_b, 4).await;

        // Verify key rotation payload
        for i in 0..payloads_a.len() {
            assert_eq!(payloads_a[i].encode_to_vec(), payloads_b[i].encode_to_vec());
        }
        verify_commit_updates_leaf_node(&group_a, &payloads_a[2]);

        // Client B sends another message to Client A, and Client A sends another message to Client B.
        group_b
            .send_message(b"Second message from B")
            .await
            .expect("send message");
        group_a
            .send_message(b"Second message from A")
            .await
            .expect("send message");

        // Group should only have 2 additional messages - no more key rotations needed.
        verify_num_payloads_in_group(&group_a, 6).await;
        verify_num_payloads_in_group(&group_b, 6).await;
    }

    async fn verify_num_payloads_in_group(
        group: &MlsGroup<FullXmtpClient>,
        num_messages: usize,
    ) -> Vec<GroupMessage> {
        let messages = group
            .client
            .api()
            .query_group_messages(group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(messages.len(), num_messages);
        messages
    }

    fn verify_commit_updates_leaf_node(group: &MlsGroup<FullXmtpClient>, payload: &GroupMessage) {
        let msgv1 = match &payload.version {
            Some(group_message::Version::V1(value)) => value,
            _ => panic!("error msgv1"),
        };

        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&msgv1.data).unwrap();
        let mls_message = match mls_message_in.extract() {
            MlsMessageBodyIn::PrivateMessage(mls_message) => mls_message,
            _ => panic!("error mls_message"),
        };

        let provider = group.client.mls_provider().unwrap();
        let decrypted_message = group
            .load_mls_group_with_lock(&provider, |mut mls_group| {
                Ok(mls_group.process_message(&provider, mls_message).unwrap())
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
