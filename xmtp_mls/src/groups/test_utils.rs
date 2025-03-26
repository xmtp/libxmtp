use crate::storage::group_message::MsgQueryArgs;

use super::{scoped_client::ScopedGroupClient, MlsGroup};
use anyhow::Result;

impl<Client: ScopedGroupClient> MlsGroup<Client> {
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<()> {
        let msg = xmtp_common::rand_string::<20>();
        self.send_message(msg.as_bytes()).await?;

        other.sync().await?;
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(
            msg.as_bytes(),
            other_msgs.pop().unwrap().decrypted_message_bytes
        );

        Ok(())
    }
}
