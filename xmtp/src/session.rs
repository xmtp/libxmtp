use crate::{
    contact::Contact,
    storage::{EncryptedMessageStoreError, PersistedSession},
    PooledSqliteConnection, Store,
};
use thiserror::Error;
use vodozemac::olm::{DecryptionError, OlmMessage, Session as OlmSession};

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("store error")]
    Store(#[from] EncryptedMessageStoreError),
    #[error("decrypt error")]
    Decrypt(#[from] DecryptionError),
    #[error("unknown error")]
    Unknown,
}

pub struct Session {
    session: OlmSession,
    persisted: PersistedSession,
}

impl Session {
    pub fn new(session: OlmSession, persisted: PersistedSession) -> Self {
        Self { session, persisted }
    }

    pub fn from_olm_session(session: OlmSession, contact: Contact) -> Result<Self, String> {
        let session_bytes = serde_json::to_vec(&session.pickle()).map_err(|e| e.to_string())?;
        let persisted = PersistedSession::new(
            session.session_id(),
            contact.wallet_address.clone(),
            contact.id(),
            session_bytes,
        );

        Ok(Self::new(session, persisted))
    }

    pub fn id(&self) -> String {
        self.session.session_id()
    }

    pub fn store(&self, into: &mut PooledSqliteConnection) -> Result<(), String> {
        self.persisted.store(into)?;
        Ok(())
    }

    pub fn session_bytes(&self) -> Result<Vec<u8>, String> {
        let res = serde_json::to_vec(&self.session.pickle()).map_err(|e| e.to_string())?;
        Ok(res)
    }

    // TODO: Replace the OlmMessage with our own message wrapper? Or leave up to the caller?
    pub fn encrypt(&mut self, plaintext: &[u8]) -> OlmMessage {
        self.session.encrypt(plaintext)
    }

    pub fn decrypt(
        &mut self,
        message: OlmMessage,
        into: &mut PooledSqliteConnection,
    ) -> Result<Vec<u8>, SessionError> {
        let res = self.session.decrypt(&message)?;
        self.persisted
            .update_session_data(self.session_bytes().unwrap(), into)
            .map_err(|_| SessionError::Unknown)?;

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use vodozemac::olm::OlmMessage;

    use crate::{
        account::{tests::test_wallet_signer, Account},
        storage::{EncryptedMessageStore, PersistedSession},
        Fetch,
    };

    #[test]
    fn round_trip_session() {
        let account_a = Account::generate(test_wallet_signer).unwrap();
        let mut account_b = Account::generate(test_wallet_signer).unwrap();

        let account_a_contact = account_a.contact();
        let account_b_contact = account_b.contact();

        let a_to_b_olm_session = account_a.create_outbound_session(account_b_contact.clone());
        let mut a_to_b_session =
            super::Session::from_olm_session(a_to_b_olm_session, account_b_contact.clone())
                .unwrap();

        let mut message_store = EncryptedMessageStore::default();
        let mut conn = message_store.conn();
        a_to_b_session.store(&mut conn).unwrap();

        let results: Vec<PersistedSession> = message_store.fetch().unwrap();
        assert_eq!(results.len(), 1);
        let initial_session_data = &results.get(0).unwrap().vmac_session_data;

        let msg = a_to_b_session.encrypt("hello".as_bytes());
        if let OlmMessage::PreKey(m) = msg.clone() {
            let mut b_to_a_olm_session = account_b
                .create_inbound_session(account_a_contact.clone(), m)
                .unwrap();

            let reply = b_to_a_olm_session.session.encrypt("hello to you");
            let decrypted_reply = a_to_b_session.decrypt(reply, &mut conn).unwrap();
            assert_eq!(decrypted_reply, "hello to you".as_bytes());

            let updated_results: Vec<PersistedSession> = message_store.fetch().unwrap();
            assert_eq!(updated_results.len(), 1);
            let updated_session_data = &updated_results.get(0).unwrap().vmac_session_data;

            assert!(initial_session_data != updated_session_data)
        } else {
            panic!("expected prekey message")
        }
    }
}
