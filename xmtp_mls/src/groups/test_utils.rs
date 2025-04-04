use super::{scoped_client::ScopedGroupClient, MlsGroup};
use xmtp_db::group_message::MsgQueryArgs;

impl<Client: ScopedGroupClient> MlsGroup<Client> {
    // Sends a mesage to other group and ensures delivery, returning sent message contents.
    pub async fn test_can_talk_with(&self, other: &Self) -> String {
        let msg = xmtp_common::rand_string::<20>();
        self.send_message(msg.as_bytes()).await.unwrap();

        // Sync to pull down the message
        other.sync().await.unwrap();
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(
            msg.as_bytes(),
            other_msgs.pop().unwrap().decrypted_message_bytes
        );

        msg
    }
}
