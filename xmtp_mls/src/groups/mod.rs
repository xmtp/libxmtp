mod intents;

use crate::{
    client::ClientError,
    configuration::CIPHERSUITE,
    storage::{
        group::{GroupMembershipState, StoredGroup},
        group_intent::{IntentKind, IntentState, NewGroupIntent, StoredGroupIntent},
        DbConnection, StorageError,
    },
    utils::{hash::sha256, time::now_ns, topic::get_group_topic},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Store,
};
use intents::SendMessageIntentData;
use openmls::{
    prelude::{
        CredentialWithKey, CryptoConfig, GroupId, MlsGroup as OpenMlsGroup, MlsGroupConfig,
        WireFormatPolicy,
    },
    prelude_test::KeyPackage,
};
use openmls_traits::OpenMlsProvider;
use thiserror::Error;
use tls_codec::Serialize;
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use self::intents::{AddMembersIntentData, IntentError, PostCommitAction};

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("group not found")]
    GroupNotFound,
    #[error("api error: {0}")]
    Api(#[from] xmtp_proto::api_client::Error),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError),
    #[error("tls serialization: {0}")]
    TlsSerialization(#[from] tls_codec::Error),
    #[error("add members: {0}")]
    AddMembers(#[from] openmls::prelude::AddMembersError<StorageError>),
    #[error("group create: {0}")]
    GroupCreate(#[from] openmls::prelude::NewGroupError<StorageError>),
    #[error("client: {0}")]
    Client(#[from] ClientError),
    #[error("generic: {0}")]
    Generic(String),
}

pub struct MlsGroup<'c, ApiClient> {
    pub group_id: Vec<u8>,
    client: &'c Client<ApiClient>,
}

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpApiClient + XmtpMlsClient,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(group_id: Vec<u8>, client: &'c Client<ApiClient>) -> Self {
        Self { client, group_id }
    }

    pub fn load_mls_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<OpenMlsGroup, GroupError> {
        let mls_group =
            match OpenMlsGroup::load(&GroupId::from_slice(&self.group_id), provider.key_store()) {
                Some(group) => group,
                None => return Err(GroupError::GroupNotFound),
            };

        Ok(mls_group)
    }

    pub fn create_and_insert(
        client: &'c Client<ApiClient>,
        membership_state: GroupMembershipState,
    ) -> Result<Self, GroupError> {
        let provider = client.mls_provider();
        let mut conn = client.store.conn()?;
        let mut mls_group = OpenMlsGroup::new(
            &provider,
            &client.identity.installation_keys,
            &build_group_config(),
            // TODO: Confirm I should be using the installation keys here
            CredentialWithKey {
                credential: client.identity.credential.clone(),
                signature_key: client.identity.installation_keys.to_public_vec().into(),
            },
        )?;

        mls_group.save(provider.key_store())?;
        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(group_id.clone(), now_ns(), membership_state);
        stored_group.store(&mut conn)?;

        Ok(Self::new(group_id, client))
    }

    pub async fn send_message(&self, message: &[u8]) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(message.to_vec()).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;
        Ok(())
    }

    pub async fn add_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let key_packages = self
            .client
            .get_key_packages_for_installation_ids(installation_ids)
            .await?;
        let intent_data: Vec<u8> = AddMembersIntentData::new(key_packages).try_into()?;
        let intent =
            NewGroupIntent::new(IntentKind::AddMembers, self.group_id.clone(), intent_data);
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;

        Ok(())
    }

    pub(crate) async fn publish_intents(&self, conn: &mut DbConnection) -> Result<(), GroupError> {
        let provider = self.client.mls_provider();
        let mut openmls_group = self.load_mls_group(&provider)?;

        let intents = self.client.store.find_group_intents(
            conn,
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;

        for intent in intents {
            // TODO: Wrap in a transaction once we can synchronize with the MLS Keystore
            let result = self.get_publish_intent_data(&provider, &mut openmls_group, &intent);
            match result {
                Ok((payload, post_commit_data)) => {
                    self.client
                        .api_client
                        .publish_to_group(vec![payload.as_slice()])
                        .await?;

                    self.client.store.set_group_intent_published(
                        conn,
                        intent.id,
                        sha256(payload.as_slice()),
                        post_commit_data,
                    )?;
                }
                Err(error) => {
                    log::error!("error getting publish intent data {:?}", error);
                }
            }
        }

        openmls_group.save(provider.key_store())?;

        Ok(())
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    fn get_publish_intent_data(
        &self,
        provider: &XmtpOpenMlsProvider,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), GroupError> {
        match intent.kind {
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // TODO: Handle pending_proposal errors and UseAfterEviction errors
                let msg = openmls_group.create_message(
                    provider,
                    &self.client.identity.installation_keys,
                    intent_data.message.as_slice(),
                )?;

                let msg_bytes = msg.tls_serialize_detached()?;
                Ok((msg_bytes, None))
            }
            IntentKind::AddMembers => {
                let intent_data =
                    AddMembersIntentData::from_bytes(intent.data.as_slice(), provider)?;

                let key_packages: Vec<KeyPackage> = intent_data
                    .key_packages
                    .iter()
                    .map(|kp| kp.inner.clone())
                    .collect();

                let (commit, welcome, _group_info) = openmls_group.add_members(
                    provider,
                    &self.client.identity.installation_keys,
                    key_packages.as_slice(),
                )?;

                let commit_bytes = commit.tls_serialize_detached()?;

                // If somehow another installation has made it into the commit, this will be missing their installation ID
                let installation_ids: Vec<Vec<u8>> = intent_data
                    .key_packages
                    .iter()
                    .map(|kp| kp.installation_id())
                    .collect();

                let post_commit_data =
                    Some(PostCommitAction::from_welcome(welcome, installation_ids)?.to_bytes());

                Ok((commit_bytes, post_commit_data))
            }
            _ => Err(GroupError::Generic("invalid intent kind".to_string())),
        }
    }

    pub fn topic(&self) -> String {
        get_group_topic(&self.group_id)
    }
}

fn build_group_config() -> MlsGroupConfig {
    MlsGroupConfig::builder()
        .crypto_config(CryptoConfig::with_default_version(CIPHERSUITE))
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(3) // Trying with 3 max past epochs for now
        .use_ratchet_tree_extension(true)
        .build()
}

#[cfg(test)]
mod tests {
    use crate::builder::ClientBuilder;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test]
    async fn test_send_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(wallet.into()).await;
        let group = client.create_group().expect("create group");
        group.send_message(b"hello").await.expect("send message");

        let topic = group.topic();

        let messages = client
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 1)
    }

    #[tokio::test]
    async fn test_add_members() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let client_2 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        client_2.register_identity().await.unwrap();
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2
                .identity
                .installation_keys
                .to_public_vec()])
            .await
            .unwrap();

        let topic = group.topic();

        let messages = client
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let group = client.create_group().expect("create group");

        let result = group
            .add_members_by_installation_id(vec![b"1234".to_vec()])
            .await;

        assert!(result.is_err());
    }
}
