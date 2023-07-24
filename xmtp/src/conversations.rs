use diesel::Connection;

use crate::{
    conversation::{ConversationError, SecretConversation},
    storage::{RefreshJob, RefreshJobKind},
    types::networking::XmtpApiClient,
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

    /**
    *  Get the `refresh_jobs` record with an id of `invites`, and obtain a lock on the row:
       Store `now()` in memory to mark the job execution start
       Fetch all messages from invite topic with timestamp > refresh_job.last_run - PADDING_TIME # PADDING TIME accounts for eventual consistency of network. Maybe 30s.
       For each message in topic:
           Save (or ignore if already exists) raw message to inbound_invite table with status of PENDING
       Update `refresh_jobs` record last_run = current_timestamp
    */
    pub fn download_invites(&self) -> Result<(), ConversationError> {
        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |conn, job| {
                let res = futures::executor::block_on(async {
                    println!("Hello world");
                    return "foo";
                });
                println!("res: {:?}", res);
                Ok(())
            })
            .unwrap();

        Ok(())
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
