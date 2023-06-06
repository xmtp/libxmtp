use crate::{
    contact::Contact,
    storage::{EncryptedMessageStore, PersistedSession},
    PooledSqliteConnection, Store,
};
use vodozemac::olm::{OlmMessage, Session as OlmSession};

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
        self.persisted.store(into)
    }

    // TODO: Replace the OlmMessage with our own message wrapper? Or leave up to the caller?
    pub fn encrypt(&mut self, plaintext: &[u8]) -> OlmMessage {
        self.session.encrypt(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        account::{tests::test_wallet_signer, Account},
        storage::{EncryptedMessageStore, PersistedSession},
        Fetch,
    };

    #[test]
    fn create_session() {
        let account_a = Account::generate(test_wallet_signer).unwrap();
        let account_b = Account::generate(test_wallet_signer).unwrap();

        let account_b_contact = account_b.contact();
        let olm_session = account_a.create_outbound_session(account_b_contact.clone());
        let session =
            super::Session::from_olm_session(olm_session, account_b_contact.clone()).unwrap();

        let mut message_store = EncryptedMessageStore::default();
        let mut conn = message_store.conn();
        session.store(&mut conn).unwrap();

        let results: Vec<PersistedSession> = message_store.fetch().unwrap();
        assert_eq!(results.len(), 1);
    }
}
