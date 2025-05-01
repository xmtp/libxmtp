use crate::groups::IntentError;
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
use openmls::prelude::Extensions;
use xmtp_db::{group_intent::IntentKind, XmtpOpenMlsProvider};

pub async fn parse_intent(
    kind: &IntentKind,
    data: &[u8],
    provider: &XmtpOpenMlsProvider,
    client: impl ScopedGroupClient,
    group: &MlsGroup,
) -> Result<Box<dyn GroupIntent>, GroupError> {
    match kind {
        IntentKind::UpdateGroupMembership => {
            let intent =
                UpdateGroupMembershipIntent::from_stored_bytes(data, provider, client, group)
                    .await?;
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

#[derive(Debug, derive_builder::Builder)]
#[builder(setter(into), build_fn(error = "IntentError"))]
pub struct PublishIntentData {
    #[builder(default = None)]
    pub staged_commit: Option<Vec<u8>>,
    #[builder(default = None)]
    pub post_commit_action: Option<Vec<u8>>,
    #[builder(default = false, setter(name = "should_push"))]
    pub should_send_push_notification: bool,
    pub payload: Vec<u8>,
}

impl PublishIntentData {
    pub fn builder() -> PublishIntentDataBuilder {
        PublishIntentDataBuilder::default()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait GroupIntent {
    async fn publish_data(
        self: Box<Self>,
        provider: &XmtpOpenMlsProvider,
        context: &XmtpMlsLocalContext,
        group: &mut MlsGroup,
        should_push: bool,
    ) -> Result<Option<PublishIntentData>, GroupError>;

    fn build_extensions(&self, group: &MlsGroup) -> Result<Extensions, GroupError>;
}
/*
/// A Generic Message
pub trait Message {
    fn is_commit(&self) -> bool;
    /// The sendable payload of the message
    fn payload(&self) -> &[u8];
    /// Prepare the message to be sent
    fn prepare(self) -> Result<GroupMessageInput, GroupError>;
}
*/
