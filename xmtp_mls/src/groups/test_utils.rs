use crate::storage::group_message::MsgQueryArgs;

use super::{scoped_client::ScopedGroupClient, MlsGroup};
use anyhow::Result;

impl<Client: ScopedGroupClient> MlsGroup<Client> {
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<()> {
        // Sync to update to the latest epoch
        self.sync().await?;
        let msg = xmtp_common::rand_string::<20>();
        self.send_message(msg.as_bytes()).await?;

        // Sync to pull down the message
        other.sync().await?;
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(
            msg.as_bytes(),
            other_msgs.pop().unwrap().decrypted_message_bytes
        );

        Ok(())
    }
}
