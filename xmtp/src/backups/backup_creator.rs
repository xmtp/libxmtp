use crate::storage::{MessageState, StorageError, StoredMessage};
use crate::Client;
use thiserror::Error;
use xmtp_proto::api_client::XmtpApiClient;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("unknown error")]
    Unknown,
}

pub struct BackupCreator<'c, A>
where
    A: XmtpApiClient,
{
    client: &'c Client<A>,
}

impl<'c, A> BackupCreator<'c, A>
where
    A: XmtpApiClient,
{
    pub fn new(client: &'c Client<A>) -> Self {
        Self { client }
    }

    fn get_messages(&self) -> Result<Vec<StoredMessage>, BackupError> {
        let conn = &mut self.client.store.conn()?;
        let messages = self.client.store.get_stored_messages(
            conn,
            Some(vec![MessageState::LocallyCommitted, MessageState::Received]),
            None,
            None,
            None,
            None,
        )?;

        Ok(messages)
    }

    pub fn create_backup(&self) {
        let messages = self.get_messages().unwrap();
    }
}
