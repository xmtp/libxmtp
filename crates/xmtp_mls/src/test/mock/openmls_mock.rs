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
use openmls::group::{GroupId, MlsGroup, MlsGroupCreateConfig};
use openmls::prelude::MlsMessageOut;
use openmls::prelude::{CredentialWithKey, KeyPackage, Welcome};
use prost::Message;
use std::collections::HashMap;
use std::sync::RwLock;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_db::group::ConversationType;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_db::{MemoryStorage, MlsMemoryStorage, MlsProviderExt, XmtpOpenMlsProvider};
use xmtp_db::{XmtpMlsStorageProvider, sql_key_store::SqlKeyStore};
use xmtp_proto::xmtp::identity::MlsCredential;

/// A minimal "other side" MLS client for tests: it drives the openmls `MlsGroup`
/// API directly to generate key packages, groups, and welcomes for the libxmtp
/// client under test. Previously wrapped openmls' own multi-client test framework
/// (`test_framework::client::Client`), which is written synchronously and does not
/// compile against the always-async openmls storage; this re-homes it onto the
/// public `MlsGroup` API with `.await`. The `installation_key` is the signer, so
/// there is no async storage read of the key pair.
pub struct BarebonesMlsClient<P: MlsProviderExt> {
    identity: Vec<u8>,
    installation_key: XmtpInstallationCredential,
    credential: CredentialWithKey,
    provider: P,
    groups: RwLock<HashMap<GroupId, MlsGroup>>,
}

#[allow(async_fn_in_trait)]
pub trait OpenMlsTestExt {
    /// Builds a fresh KeyPackage and stores its reference in the local db.
    async fn key_package(&self) -> KeyPackage;

    /// Create a group in mls memory, returning its id.
    async fn create_mls_group(&self, members: &[&str]) -> GroupId;

    /// Adds an anonymous member to the group; returns their KP and the welcome.
    async fn add_member(&self, group_id: &GroupId) -> (KeyPackage, Welcome);

    /// Join an anonymous group; returns our key package and a welcome to join it.
    async fn join_group(&self) -> (KeyPackage, MlsMessageOut);
}

fn credential_with_key(
    identity: &str,
    installation_key: &XmtpInstallationCredential,
) -> CredentialWithKey {
    CredentialWithKey {
        credential: create_credential(identity).unwrap(),
        signature_key: installation_key.clone().into(),
    }
}

/// Create an owned anonymous client backed by fresh in-memory MLS storage.
pub fn gen_client(identity: &str) -> BarebonesMlsClient<XmtpOpenMlsProvider<MlsMemoryStorage>> {
    let store = SqlKeyStore::new(MemoryStorage::new());
    let installation_key = XmtpInstallationCredential::new();
    let credential = credential_with_key(identity, &installation_key);
    BarebonesMlsClient {
        identity: identity.as_bytes().to_vec(),
        installation_key,
        credential,
        provider: XmtpOpenMlsProvider::new(store),
        groups: RwLock::new(HashMap::new()),
    }
}

/// Create a client backed by the given storage (e.g. the mock context's store).
pub fn create_mls_client<S: XmtpMlsStorageProvider>(
    store: &S,
) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, S>> {
    let installation_key = XmtpInstallationCredential::new();
    let credential = credential_with_key("alice", &installation_key);
    BarebonesMlsClient {
        identity: b"alice".to_vec(),
        installation_key,
        credential,
        provider: XmtpOpenMlsProviderRef::new(store),
        groups: RwLock::new(HashMap::new()),
    }
}

impl MockStoreAndContext {
    /// Create an MLS client backed by this context's mock storage.
    pub fn mls_client(
        &self,
    ) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, xmtp_db::sql_key_store::mock::MockSqlKeyStore>>
    {
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
    let protected_metadata =
        build_protected_metadata_extension(creator_inbox, ConversationType::Group, None)?;
    let mutable_metadata =
        build_mutable_metadata_extension_default(creator_inbox, Default::default())?;
    let group_membership = build_starting_group_membership_extension(creator_inbox, 0);
    let mutable_permissions = build_mutable_permissions_extension(Default::default())?;
    build_group_config(
        protected_metadata,
        mutable_metadata,
        group_membership,
        mutable_permissions,
    )
}

impl<P: MlsProviderExt> OpenMlsTestExt for BarebonesMlsClient<P> {
    async fn key_package(&self) -> KeyPackage {
        XmtpKeyPackage::builder()
            .inbox_id(String::from_utf8_lossy(&self.identity))
            .credential(self.credential.credential.clone())
            .installation_keys(self.installation_key.clone())
            .build(&self.provider, false)
            .await
            .unwrap()
            .key_package
    }

    async fn create_mls_group(&self, members: &[&str]) -> GroupId {
        let config = generate_group_config("alice", members).unwrap();
        let group = MlsGroup::new(
            &self.provider,
            &self.installation_key,
            &config,
            self.credential.clone(),
        )
        .await
        .unwrap();
        let group_id = group.group_id().clone();
        self.groups
            .write()
            .unwrap()
            .insert(group_id.clone(), group);
        group_id
    }

    async fn add_member(&self, group_id: &GroupId) -> (KeyPackage, Welcome) {
        let new_member = gen_client(&xmtp_common::rand_string::<4>());
        let kp = new_member.key_package().await;
        // Own the group across the await so we do not hold the lock over it.
        let mut group = self.groups.write().unwrap().remove(group_id).unwrap();
        let (_commit, welcome, _group_info) = group
            .add_members(&self.provider, &self.installation_key, std::slice::from_ref(&kp))
            .await
            .unwrap();
        self.groups
            .write()
            .unwrap()
            .insert(group_id.clone(), group);
        (kp, welcome.into_welcome().expect("expected a welcome message"))
    }

    async fn join_group(&self) -> (KeyPackage, MlsMessageOut) {
        let anon = gen_client(&format!("anon-{}", xmtp_common::rand_string::<4>()));
        let inbox_id = String::from_utf8_lossy(&self.identity).to_string();
        let group_id = anon.create_mls_group(&[&inbox_id]).await;
        tracing::info!("created anon mock mls group {}", hex::encode(group_id.as_slice()));
        let kp = self.key_package().await;

        let mut group = anon.groups.write().unwrap().remove(&group_id).unwrap();
        let mut membership = GroupMembership::new();
        for m in group.members() {
            let c: MlsCredential =
                MlsCredential::decode(m.credential.serialized_content()).unwrap();
            membership.members.insert(c.inbox_id, 0);
        }
        membership.members.insert(inbox_id, 0);
        let mut new_extensions = group.extensions().clone();
        new_extensions
            .add_or_replace(build_group_membership_extension(&membership))
            .unwrap();

        let (_commit, welcome, _group_info) = group
            .update_group_membership(
                &anon.provider,
                &anon.installation_key,
                std::slice::from_ref(&kp),
                &[],
                new_extensions,
            )
            .await
            .unwrap();
        (kp, welcome.unwrap())
    }
}
