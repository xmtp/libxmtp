#![allow(unused)]
#![allow(clippy::unwrap_used)]

use crate::{
    client::ClientError,
    groups::{GroupError, MlsGroup},
};
use thiserror::Error;
use xmtp_api::XmtpApi;
use xmtp_db::{group_message::MsgQueryArgs, XmtpDb};

#[derive(Error, Debug)]
pub enum TestError {
    #[error("{0}")]
    Generic(String),
    #[error(transparent)]
    Group(#[from] GroupError),
    #[error(transparent)]
    Client(#[from] ClientError),
}

impl<ApiClient, Db> MlsGroup<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    // Sends a mesage to other group and ensures delivery, returning sent message contents.
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<String, TestError> {
        let msg = xmtp_common::rand_string::<20>();
        self.sync().await?;
        self.send_message(msg.as_bytes()).await?;

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
}
