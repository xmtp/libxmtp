#![allow(unused)]
#![allow(clippy::unwrap_used)]

use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, send_message_opts::SendMessageOpts},
};
use thiserror::Error;
use xmtp_api::{ApiError, XmtpApi};
use xmtp_api_d14n::protocol::{EnvelopeError, XmtpQuery};
use xmtp_common::RetryableError;
use xmtp_db::{XmtpDb, group_message::MsgQueryArgs};
use xmtp_proto::types::{GroupMessage, TopicKind};

#[derive(Error, Debug)]
pub enum TestError {
    #[error("{0}")]
    Generic(String),
    #[error(transparent)]
    Group(#[from] GroupError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Api(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
}

impl RetryableError for TestError {
    fn is_retryable(&self) -> bool {
        true
    }
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    // Sends a mesage to other group and ensures delivery, returning sent message contents.
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<String, TestError> {
        let msg = xmtp_common::rand_string::<20>();
        self.sync().await?;
        self.send_message(msg.as_bytes(), SendMessageOpts::default())
            .await?;

        // Sync to pull down the message
        other.sync().await?;
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default())?;
        if msg.as_bytes() != other_msgs.pop().unwrap().decrypted_message_bytes {
            return Err(TestError::Generic(
                "Sent message was not received.".to_string(),
            ));
        }

        Ok(msg)
    }

    pub async fn test_get_last_message_from_network(&self) -> Result<GroupMessage, TestError> {
        let mut messages = self
            .context
            .api()
            .query_at(TopicKind::GroupMessagesV1.create(&self.group_id), None)
            .await
            .map_err(xmtp_api::dyn_err)?
            .group_messages()?;

        let last_message = messages
            .pop()
            .ok_or(TestError::Generic("No messages found".to_string()))?;

        Ok(last_message)
    }
}
