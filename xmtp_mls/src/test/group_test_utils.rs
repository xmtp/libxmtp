#![allow(unused)]
#![allow(clippy::unwrap_used)]

use crate::groups::{GroupError, MlsGroup};
use xmtp_api::XmtpApi;
use xmtp_db::{group_message::MsgQueryArgs, XmtpDb};

impl<ApiClient, Db> MlsGroup<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    // Sends a mesage to other group and ensures delivery, returning sent message contents.
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<String, GroupError> {
        let msg = xmtp_common::rand_string::<20>();
        self.send_message(msg.as_bytes()).await?;

        // Sync to pull down the message
        other.sync().await?;
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(
            msg.as_bytes(),
            other_msgs.pop().unwrap().decrypted_message_bytes
        );

        Ok(msg)
    }
}
