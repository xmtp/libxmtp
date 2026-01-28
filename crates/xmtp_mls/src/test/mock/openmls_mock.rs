use super::*;
use crate::groups::group_membership::GroupMembership;
use crate::groups::{GroupError, build_group_membership_extension};
use crate::identity::XmtpKeyPackage;
use crate::{
    groups::{
        build_group_config, build_mutable_metadata_extension_default,
        build_mutable_permissions_extension, build_protected_metadata_extension,
        build_starting_group_membership_extension,
    },
    identity::create_credential,
};
use openmls::group::{GroupId, MlsGroupCreateConfig};
use openmls::prelude::MlsMessageOut;
use openmls::prelude::{CredentialWithKey, KeyPackage, Welcome};
use openmls::storage::OpenMlsProvider;
use openmls::test_utils::test_framework::ActionType;
use openmls::test_utils::test_framework::client::Client;
use openmls::test_utils::test_framework::errors::ClientError;
use prost::Message;
use std::collections::HashMap;
use std::sync::RwLock;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_cryptography::configuration::CIPHERSUITE;
use xmtp_db::group::ConversationType;
use xmtp_db::sql_key_store::SqlKeyStoreError;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_db::{MemoryStorage, MlsMemoryStorage, MlsProviderExt, XmtpOpenMlsProvider};
use xmtp_db::{
    XmtpMlsStorageProvider,
    sql_key_store::{SqlKeyStore, mock::MockSqlKeyStore},
};
use xmtp_proto::xmtp::identity::MlsCredential;

pub struct BarebonesMlsClient<P: OpenMlsProvider> {
    installation_key: XmtpInstallationCredential,
    client: Client<P>,
}

pub trait OpenMlsTestExt {
    /// Builds a fresh KeyPackage and stores its reference in the local db
    fn key_package(&self) -> Result<KeyPackage, ClientError<SqlKeyStoreError>>;

    /// Create a group in mls memory
    fn create_mls_group(&self, members: &[&str]) -> Result<GroupId, ClientError<SqlKeyStoreError>>;

    /// Adds an anonymous member to [GroupId]
    /// Returns KP of that member and the welcome
    fn add_member(&self, group_id: &GroupId) -> (KeyPackage, Welcome);

    /// Join an anonymous group
    /// Returns our key package used to join the group, and a welcome
    /// to join the group.
    fn join_group(&self) -> (KeyPackage, MlsMessageOut);
}

/// create an owned anonymous client
pub fn gen_client(identity: &str) -> BarebonesMlsClient<XmtpOpenMlsProvider<MlsMemoryStorage>> {
    let store = SqlKeyStore::new(MemoryStorage::new());
    let mut credentials = HashMap::new();
    let installation_key = XmtpInstallationCredential::new();
    let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
    key_pair.store(&store).unwrap();
    let signature_key = installation_key.clone().into();
    let credential = CredentialWithKey {
        credential: create_credential(identity).unwrap(),
        signature_key,
    };
    credentials.insert(CIPHERSUITE, credential);
    let client = Client::<_> {
        identity: b"alice".to_vec(),
        credentials,
        provider: XmtpOpenMlsProvider::new(store),
        groups: RwLock::new(HashMap::new()),
    };

    BarebonesMlsClient {
        installation_key,
        client,
    }
}

pub fn create_mls_client<S: XmtpMlsStorageProvider>(
    store: &S,
) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, S>> {
    let mut credentials = HashMap::new();
    let installation_key = XmtpInstallationCredential::new();
    let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
    key_pair.store(store).unwrap();
    let signature_key = installation_key.clone().into();
    let credential = CredentialWithKey {
        credential: create_credential("alice").unwrap(),
        signature_key,
    };
    credentials.insert(CIPHERSUITE, credential);
    BarebonesMlsClient {
        installation_key,
        client: Client::<_> {
            identity: b"alice".to_vec(),
            credentials,
            provider: XmtpOpenMlsProviderRef::new(store),
            groups: RwLock::new(HashMap::new()),
        },
    }
}

impl MockStoreAndContext {
    /// Create an MLS client with an XMTP Installation Key
    /// Stores the Key package in OpenMls Memory storage
    /// Adds credential to client
    pub fn mls_client(&self) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, MockSqlKeyStore>> {
        create_mls_client(&self.mls_storage)
    }
}

fn generate_group_config(
    creator_inbox: &str,
    members: &[&str],
) -> Result<MlsGroupCreateConfig, GroupError> {
    let mut membership = GroupMembership::new();
    membership.add(creator_inbox.to_string(), 0);
    members
        .iter()
        .for_each(|m| membership.add(m.to_string(), 0));
    let _group_membership = build_group_membership_extension(&membership);
    let protected_metadata =
        build_protected_metadata_extension(creator_inbox, ConversationType::Group, None)?;
    let mutable_metadata =
        build_mutable_metadata_extension_default(creator_inbox, Default::default())?;
    let group_membership = build_starting_group_membership_extension(creator_inbox, 0);
    let mutable_permissions = build_mutable_permissions_extension(Default::default())?;
    let group_config = build_group_config(
        protected_metadata,
        mutable_metadata,
        group_membership,
        mutable_permissions,
    )?;
    Ok(group_config)
}

impl<P: MlsProviderExt> OpenMlsTestExt for BarebonesMlsClient<P> {
    fn key_package(&self) -> Result<KeyPackage, ClientError<SqlKeyStoreError>> {
        let cred = self.client.credentials.get(&CIPHERSUITE).unwrap();
        let cred = &cred.credential;
        Ok(XmtpKeyPackage::builder()
            .inbox_id(String::from_utf8_lossy(&self.client.identity))
            .credential(cred.clone())
            .installation_keys(self.installation_key.clone())
            .build(&self.client.provider, false)
            .unwrap()
            .key_package)
    }

    fn create_mls_group(&self, members: &[&str]) -> Result<GroupId, ClientError<SqlKeyStoreError>> {
        let config = generate_group_config("alice", members).unwrap();
        self.client.create_group(config, CIPHERSUITE)
    }

    fn add_member(&self, group_id: &GroupId) -> (KeyPackage, Welcome) {
        let new_member = gen_client(&xmtp_common::rand_string::<4>());
        let kp = new_member.key_package().unwrap();
        let (_, welcome, _) = self
            .client
            .add_members(ActionType::Commit, group_id, std::slice::from_ref(&kp))
            .unwrap();
        (kp, welcome.unwrap())
    }

    fn join_group(&self) -> (KeyPackage, MlsMessageOut) {
        let anon = gen_client(&format!("anon-{}", xmtp_common::rand_string::<4>()));
        let inbox_id = String::from_utf8_lossy(&self.client.identity);
        let group_id = anon.create_mls_group(&[&inbox_id]).unwrap();
        tracing::info!(
            "created anon mock mls group {}",
            hex::encode(group_id.as_slice())
        );
        let kp = self.key_package().unwrap();

        let mut groups = anon.client.groups.write().unwrap();
        let mls_group = groups.get_mut(&group_id).unwrap();

        let mut membership = GroupMembership::new();
        for m in mls_group.members() {
            let c: MlsCredential =
                MlsCredential::decode(m.credential.serialized_content()).unwrap();
            membership.members.insert(c.inbox_id, 0);
        }
        membership.members.insert(inbox_id.to_string(), 0);
        // Update the extensions to have the new GroupMembership
        let mut new_extensions = mls_group.extensions().clone();
        new_extensions
            .add_or_replace(build_group_membership_extension(&membership))
            .unwrap();

        let (_commit, welcome, _) = mls_group
            .update_group_membership(
                &anon.client.provider,
                &anon.installation_key,
                std::slice::from_ref(&kp),
                &[],
                new_extensions,
            )
            .unwrap();
        (kp, welcome.unwrap())
    }
}
