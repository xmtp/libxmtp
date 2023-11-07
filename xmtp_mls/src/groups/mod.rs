mod intents;

use crate::{
    storage::{
        group::{GroupMembershipState, StoredGroup},
        group_intent::{IntentKind, IntentState, NewGroupIntent},
        DbConnection, StorageError,
    },
    utils::{hash::sha256, time::now_ns},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Store,
};
use intents::SendMessageIntentData;
use openmls::{
    prelude::{GroupId, MlsGroup as OpenMlsGroup},
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
    #[error("client error {0}")]
    Client(#[from] crate::client::ClientError),
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

    pub fn new_and_insert(
        client: &'c Client<ApiClient>,
        group_id: Vec<u8>,
        membership_state: GroupMembershipState,
    ) -> Result<Self, GroupError> {
        let mut conn = client.store.conn()?;
        let stored_group = StoredGroup::new(group_id.clone(), now_ns(), membership_state);
        stored_group.store(&mut conn)?;

        Ok(Self::new(group_id, client))
    }

    pub fn send_message(&self, message: &[u8]) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(message.to_vec()).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(&mut conn)?;

        Ok(())
    }

    pub(crate) async fn publish_intents(&self, conn: &mut DbConnection) -> Result<(), GroupError> {
        // TODO: Wrap in a transaction
        let provider = self.client.mls_provider();
        let mut openmls_group = self.load_mls_group(&provider)?;

        let intents = self.client.store.find_group_intents(
            conn,
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;

        // TODO: Re-organize to batch publish
        for intent in intents {
            let (payload, post_commit_data): (Vec<u8>, Option<Vec<u8>>) = match intent.kind {
                IntentKind::SendMessage => {
                    // We can safely assume all SendMessage intents have data
                    let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                    // TODO: Handle pending_proposal errors and UseAfterEviction errors
                    let msg = openmls_group.create_message(
                        &provider,
                        &self.client.identity.installation_keys,
                        intent_data.message.as_slice(),
                    )?;

                    let msg_bytes = msg.tls_serialize_detached()?;
                    (msg_bytes, None)
                }
                IntentKind::AddMembers => {
                    let intent_data =
                        AddMembersIntentData::from_bytes(intent.data.as_slice(), &provider)?;

                    let key_packages: Vec<KeyPackage> = intent_data
                        .key_packages
                        .iter()
                        .map(|kp| kp.inner.clone())
                        .collect();

                    let (commit, welcome, _) = openmls_group.add_members(
                        &provider,
                        &self.client.identity.installation_keys,
                        &key_packages.as_slice(),
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

                    (commit_bytes, post_commit_data)
                }
                _ => return Err(GroupError::GroupNotFound),
            };

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

        openmls_group.save(provider.key_store())?;

        Ok(())
    }

    pub fn topic(&self) -> String {
        format!("/xmtp/3/g-{}", hex::encode(&self.group_id))
    }
}
