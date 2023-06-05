use crate::{
    contact::Contact,
    storage::{EncryptedMessageStore, PersistedSession},
    Store,
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

    pub fn store(&mut self, into: &mut EncryptedMessageStore) -> Result<(), String> {
        self.persisted.store(into)
    }

    // TODO: Replace the OlmMessage with our own message wrapper? Or leave up to the caller?
    pub fn encrypt(&mut self, plaintext: &[u8]) -> OlmMessage {
        self.session.encrypt(plaintext)
    }
}
