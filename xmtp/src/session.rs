use vodozemac::olm::{OlmMessage, Session as OlmSession};

pub struct Session {
    session: OlmSession,
}

impl Session {
    pub fn new(session: OlmSession) -> Self {
        Self { session }
    }

    pub fn id(&self) -> String {
        self.session.session_id()
    }

    // TODO: Replace the OlmMessage with our own message wrapper? Or leave up to the caller?
    pub fn encrypt(&mut self, plaintext: &[u8]) -> OlmMessage {
        self.session.encrypt(plaintext)
    }
}
