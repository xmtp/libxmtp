use futures::TryFutureExt;
use xmtp_proto::xmtp::message_api::v1::QueryRequest;

use crate::{
    conversation::{ConversationError, SecretConversation},
    invitation::Invitation,
    storage::{RefreshJob, RefreshJobKind, StorageError},
    types::networking::XmtpApiClient,
    utils::build_user_invite_topic,
    Client,
};

const PADDING_TIME: i64 = 30 * 1000 * 1000;

pub struct Conversations<'c, A>
where
    A: XmtpApiClient,
{
    client: &'c Client<A>,
}

impl<'c, A> Conversations<'c, A>
where
    A: XmtpApiClient,
{
    pub fn new(client: &'c Client<A>) -> Self {
        Self { client }
    }

    pub async fn new_secret_conversation(
        &self,
        wallet_address: String,
    ) -> Result<SecretConversation<A>, ConversationError> {
        let contacts = self.client.get_contacts(wallet_address.as_str()).await?;
        SecretConversation::new(self.client, wallet_address, contacts)
    }

    pub fn download_invites(&self) -> Result<Vec<Invitation>, ConversationError> {
        let mut invites = Vec::new();

        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |_, job| {
                let downloaded = futures::executor::block_on(
                    self.do_download(self.get_start_time(job).unsigned_abs()),
                )
                .map_err(|_| StorageError::Unknown)?;
                for invite in downloaded {
                    invites.push(invite)
                }

                Ok(())
            })
            .unwrap();

        Ok(invites)
    }

    async fn do_download(&self, start_time: u64) -> Result<Vec<Invitation>, ConversationError> {
        let my_contact = self.client.account.contact();
        let response = self
            .client
            .api_client
            .query(QueryRequest {
                content_topics: vec![build_user_invite_topic(my_contact.installation_id())],
                start_time_ns: start_time,
                end_time_ns: 0,
                // TODO: Pagination
                paging_info: None,
            })
            .map_err(|_| ConversationError::Unknown)
            .await?;

        let mut invites = vec![];
        for envelope in response.envelopes {
            let invite = envelope.message.try_into();
            match invite {
                Ok(invite) => invites.push(invite),
                _ => continue,
            }
        }

        Ok(invites)
    }

    fn get_start_time(&self, job: RefreshJob) -> i64 {
        // Adjust for padding and ensure start_time > 0
        std::cmp::max(job.last_run - PADDING_TIME, 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{conversations::Conversations, ClientBuilder};

    #[tokio::test]
    async fn create_secret_conversation() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();
        let mut bob_client = ClientBuilder::new_test().build().unwrap();
        bob_client.init().await.unwrap();

        let conversations = Conversations::new(&alice_client);
        let conversation = conversations
            .new_secret_conversation(bob_client.wallet_address().to_string())
            .await
            .unwrap();

        assert_eq!(conversation.peer_address(), bob_client.wallet_address());
        conversation.initialize().await.unwrap();
    }

    #[tokio::test]
    async fn download_invites() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();

        let invites = Conversations::new(&alice_client).download_invites();
        assert!(invites.is_ok());
    }
}
