use crate::groups::{mls_ext::GroupIntent, mls_ext::PublishIntentData};
use prost::Message;
use tls_codec::Serialize;

use super::IntentError;
use crate::GroupError;
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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl GroupIntent for SendMessageIntentData {
    async fn publish_data(
        self: Box<Self>,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut openmls::prelude::MlsGroup,
        should_push: bool,
    ) -> Result<Option<PublishIntentData>, crate::groups::GroupError> {
        let msg = group.create_message(
            provider,
            &context.identity.installation_keys,
            self.message.as_slice(),
        )?;

        PublishIntentData::builder()
            .payload(msg.tls_serialize_detached()?)
            .should_push(should_push)
            .build()
            .map_err(GroupError::from)
            .map(Option::Some)
    }
}

impl From<SendMessageIntentData> for Vec<u8> {
    fn from(intent: SendMessageIntentData) -> Self {
        intent.to_bytes()
    }
}
