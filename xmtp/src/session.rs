use crate::{
    contact::Contact,
    storage::{DbConnection, StorageError, StoredSession},
    Save, Store,
};
use thiserror::Error;
use vodozemac::olm::{DecryptionError, OlmMessage, Session as OlmSession};

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("storage error")]
    Storage(#[from] StorageError),
    #[error("decrypt error")]
    Decrypt(#[from] DecryptionError),
    #[error("serialization error")]
    Serialization(#[from] serde_json::Error),
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug)]
pub struct SessionManager {
    user_address: String,
    peer_installation_id: String,
    session: OlmSession,
}

impl SessionManager {
    pub fn new(session: OlmSession, peer_installation_id: String, user_address: String) -> Self {
        Self {
            user_address,
            session,
            peer_installation_id,
        }
    }

    pub fn from_olm_session(session: OlmSession, contact: &Contact) -> Result<Self, String> {
        Ok(Self::new(
            session,
            contact.installation_id(),
            contact.wallet_address.clone(),
        ))
    }

    pub fn id(&self) -> String {
        self.session.session_id()
    }

    pub fn user_address(&self) -> String {
        self.user_address.clone()
    }

    pub fn installation_id(&self) -> String {
        self.peer_installation_id.clone()
    }

    pub fn session_bytes(&self) -> Result<Vec<u8>, SessionError> {
        let res = serde_json::to_vec(&self.session.pickle())?;
        Ok(res)
    }

    // TODO: Replace the OlmMessage with our own message wrapper? Or leave up to the caller?
    pub fn encrypt(&mut self, plaintext: &[u8]) -> OlmMessage {
        self.session.encrypt(plaintext)
    }

    pub fn decrypt(
        &mut self,
        message: OlmMessage,
        into: &mut DbConnection,
    ) -> Result<Vec<u8>, SessionError> {
        let res = self.session.decrypt(&message)?;

        self.save(into)?;

        Ok(res)
    }

    pub fn has_received_message(&self) -> bool {
        self.session.has_received_message()
    }
}

impl Store<DbConnection> for SessionManager {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        StoredSession::try_from(self)?.store(into)
    }
}

impl Save<DbConnection> for SessionManager {
    fn save(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        StoredSession::try_from(self)?.save(into)
    }
}

impl TryFrom<&StoredSession> for SessionManager {
    type Error = StorageError;
    fn try_from(value: &StoredSession) -> Result<Self, StorageError> {
        let pickle = serde_json::from_slice(&value.vmac_session_data)
            .map_err(|_| StorageError::SerializationError)?;

        Ok(Self::new(
            OlmSession::from_pickle(pickle),
            value.peer_installation_id.clone(),
            value.user_address.clone(),
        ))
    }
}

impl TryFrom<&SessionManager> for StoredSession {
    type Error = StorageError;

    fn try_from(value: &SessionManager) -> Result<Self, Self::Error> {
        Ok(StoredSession::new(
            value.session.session_id(),
            value.peer_installation_id.clone(),
            // TODO: Better error handling approach. StoreError and SessionError end up being dependent on eachother
            value
                .session_bytes()
                .map_err(|_| StorageError::SerializationError)?,
            value.user_address.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use vodozemac::olm::OlmMessage;

    use crate::{
        account::{tests::test_wallet_signer, Account},
        storage::{EncryptedMessageStore, StoredSession},
        Fetch, Store,
    };

    #[test]
    fn round_trip_session() {
        let account_a = Account::generate(test_wallet_signer).unwrap();
        let account_b = Account::generate(test_wallet_signer).unwrap();

        let account_a_contact = account_a.contact();
        let account_b_contact = account_b.contact();

        let a_to_b_olm_session = account_a.create_outbound_session(&account_b_contact);
        let mut a_to_b_session =
            super::SessionManager::from_olm_session(a_to_b_olm_session, &account_b_contact)
                .unwrap();

        let message_store = &EncryptedMessageStore::default();
        let conn = &mut message_store.conn().unwrap();

        a_to_b_session.store(conn).unwrap();

        let results: Vec<StoredSession> = conn.fetch_all().unwrap();
        assert_eq!(results.len(), 1);
        let initial_session_data = &results.get(0).unwrap().vmac_session_data;

        let msg = a_to_b_session.encrypt("hello".as_bytes());
        if let OlmMessage::PreKey(m) = msg.clone() {
            let mut b_to_a_olm_session = account_b
                .create_inbound_session(&account_a_contact, m)
                .unwrap();

            let reply = b_to_a_olm_session.session.encrypt("hello to you");
            let decrypted_reply = a_to_b_session.decrypt(reply, conn).unwrap();
            assert_eq!(decrypted_reply, "hello to you".as_bytes());

            let updated_results: Vec<StoredSession> = conn.fetch_all().unwrap();
            assert_eq!(updated_results.len(), 1);
            let updated_session_data = &updated_results.get(0).unwrap().vmac_session_data;

            assert!(initial_session_data != updated_session_data)
        } else {
            panic!("expected prekey message")
        }
    }
}
