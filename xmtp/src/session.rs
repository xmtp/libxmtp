use crate::{
    contact::Contact,
    storage::{EncryptedMessageStore, Session, StorageError},
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

pub struct SessionManager {
    session: OlmSession,
    persisted: Session,
}

impl SessionManager {
    pub fn new(session: OlmSession, persisted: Session) -> Self {
        Self { session, persisted }
    }

    pub fn from_olm_session(session: OlmSession, contact: Contact) -> Result<Self, String> {
        let session_bytes = serde_json::to_vec(&session.pickle()).map_err(|e| e.to_string())?;
        let persisted = Session::new(
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

    pub fn store(&self, into: &EncryptedMessageStore) -> Result<(), StorageError> {
        self.persisted.store(into)?;
        Ok(())
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
        into: &EncryptedMessageStore,
    ) -> Result<Vec<u8>, SessionError> {
        let res = self.session.decrypt(&message)?;
        let session_bytes = self.session_bytes()?;
        // TODO: Stop mutating/storing the persisted session and just build on demand
        self.persisted.vmac_session_data = session_bytes;
        self.persisted.save(into)?;

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use vodozemac::olm::OlmMessage;

    use crate::{
        account::{tests::test_wallet_signer, Account},
        storage::{EncryptedMessageStore, Session},
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
            super::SessionManager::from_olm_session(a_to_b_olm_session, account_b_contact.clone())
                .unwrap();

        let message_store = &EncryptedMessageStore::default();
        a_to_b_session.store(message_store).unwrap();

        let results: Vec<Session> = message_store.fetch().unwrap();
        assert_eq!(results.len(), 1);
        let initial_session_data = &results.get(0).unwrap().vmac_session_data;

        let msg = a_to_b_session.encrypt("hello".as_bytes());
        if let OlmMessage::PreKey(m) = msg.clone() {
            let mut b_to_a_olm_session = account_b
                .create_inbound_session(account_a_contact.clone(), m)
                .unwrap();

            let reply = b_to_a_olm_session.session.encrypt("hello to you");
            let decrypted_reply = a_to_b_session.decrypt(reply, message_store).unwrap();
            assert_eq!(decrypted_reply, "hello to you".as_bytes());

            let updated_results: Vec<Session> = message_store.fetch().unwrap();
            assert_eq!(updated_results.len(), 1);
            let updated_session_data = &updated_results.get(0).unwrap().vmac_session_data;

            assert!(initial_session_data != updated_session_data)
        } else {
            panic!("expected prekey message")
        }
    }
}
