use crate::{
    client::XmtpMlsLocalContext,
    groups::{
        intents::{
            KeyUpdateIntent, SendMessageIntentData, UpdateAdminListIntentData,
            UpdateGroupMembershipIntent, UpdateMetadataIntentData, UpdatePermissionIntentData,
        },
        scoped_client::ScopedGroupClient,
        GroupError,
    },
};
use openmls::group::MlsGroup;
use xmtp_db::{
    group_intent::{IntentKind, StoredGroupIntent},
    XmtpOpenMlsProvider,
};
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;

pub fn parse_intent(kind: &IntentKind, data: &[u8]) -> Result<Box<dyn GroupIntent>, GroupError> {
    match kind {
        IntentKind::UpdateGroupMembership => {
            let intent = UpdateGroupMembershipIntent::try_from(data)?;
            Ok(Box::new(intent))
        }
        IntentKind::SendMessage => {
            let intent = SendMessageIntentData::from_bytes(data)?;
            Ok(Box::new(intent))
        }
        IntentKind::KeyUpdate => Ok(Box::new(KeyUpdateIntent)),
        IntentKind::MetadataUpdate => {
            let intent = UpdateMetadataIntentData::try_from(data.to_vec())?;
            Ok(Box::new(intent))
        }
        IntentKind::UpdateAdminList => {
            let intent = UpdateAdminListIntentData::try_from(data.to_vec())?;
            Ok(Box::new(intent))
        }
        IntentKind::UpdatePermission => {
            let intent = UpdatePermissionIntentData::try_from(data.to_vec())?;
            Ok(Box::new(intent))
        }
    }
}

#[derive(Debug)]
pub struct PublishIntentData {
    pub(super) staged_commit: Option<Vec<u8>>,
    pub(super) post_commit_action: Option<Vec<u8>>,
    pub(super) payload_to_publish: Vec<u8>,
    pub(super) should_send_push_notification: bool,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait GroupIntent {
    async fn publish_data(
        self,
        provider: &XmtpOpenMlsProvider,
        context: &XmtpMlsLocalContext,
        group: &mut MlsGroup,
        should_push: bool,
    ) -> Result<Option<PublishIntentData>, GroupError>;
}

/// A Generic Message
pub trait Message {
    fn is_commit(&self) -> bool;
    /// The sendable payload of the message
    fn payload(&self) -> &[u8];
    /// Prepare the message to be sent
    fn prepare(self) -> Result<GroupMessageInput, GroupError>;
}
