use prost::Message;

use crate::groups::{mls_ext::GroupIntent, mls_sync::PublishIntentData};

use super::IntentError;
use xmtp_proto::xmtp::mls::database::{
    send_message_data::{Version as SendMessageVersion, V1 as SendMessageV1},
    SendMessageData,
};

#[derive(Debug, Clone)]
pub struct SendMessageIntentData {
    pub message: Vec<u8>,
}

impl SendMessageIntentData {
    pub fn new(message: Vec<u8>) -> Self {
        Self { message }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        SendMessageData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.clone(),
            })),
        }
        .encode_to_vec()
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessageData::decode(data)?;
        let payload_bytes = match msg.version {
            Some(SendMessageVersion::V1(v1)) => v1.payload_bytes,
            None => return Err(IntentError::MissingPayload),
        };

        Ok(Self::new(payload_bytes))
    }
}

impl GroupIntent for SendMessageIntentData {
    async fn publish_data(
        &self,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        client: impl crate::groups::scoped_client::ScopedGroupClient,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut openmls::prelude::MlsGroup,
    ) -> Result<Option<crate::groups::mls_sync::PublishIntentData>, crate::groups::GroupError> {
        let msg = group.create_message(
            provider,
            context.identity.installation_keys,
            self.message.as_slice(),
        )?;

        Ok(Some(PublishIntentData {
            payload_to_publish: msg.tls_serialize_detached()?,
            post_commit_action: None,
            staged_commit: None,
            should_send_push_notification: intent.should_push,
        }))
    }
}

impl From<SendMessageIntentData> for Vec<u8> {
    fn from(intent: SendMessageIntentData) -> Self {
        intent.to_bytes()
    }
}
