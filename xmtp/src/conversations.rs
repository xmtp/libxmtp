use crate::{
    conversation::{ConversationError, SecretConversation},
    invitation::Invitation,
    storage::{RefreshJob, RefreshJobKind, StorageError},
    types::networking::XmtpApiClient,
    Client,
};

const PADDING_TIME_NS: i64 = 30 * 1000 * 1000 * 1000;

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

    pub fn save_invites(&self) -> Result<Vec<Invitation>, ConversationError> {
        let mut invites = Vec::new();

        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |_, job| {
                let downloaded = futures::executor::block_on(
                    self.client
                        .download_invites(self.get_start_time(job).unsigned_abs()),
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

    fn get_start_time(&self, job: RefreshJob) -> i64 {
        // Adjust for padding and ensure start_time > 0
        std::cmp::max(job.last_run - PADDING_TIME_NS, 0)
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
    async fn save_invites() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();

        let invites = Conversations::new(&alice_client).save_invites();
        assert!(invites.is_ok());
    }
}
