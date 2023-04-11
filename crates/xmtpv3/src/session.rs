use anyhow::Result;
use vodozemac::olm::{OlmMessage, Session};

// Wrapper struct for Olm sessions with additional metadata
pub struct VoodooSession {
    pub session: Session,
    // NOTE: this does not save the first plaintext encoded in the
    // outbound session
    // TODO: instead of storing pure plaintext, we should store higher level structs
    pub my_messages: Vec<String>,
    pub their_messages: Vec<String>,
}

impl VoodooSession {
    pub fn new(olm_session: Session) -> Self {
        Self {
            session: olm_session,
            my_messages: Vec::new(),
            their_messages: Vec::new(),
        }
    }

    pub fn encrypt(&mut self, plaintext: &str) -> OlmMessage {
        let message = self.session.encrypt(plaintext);
        self.my_messages.push(plaintext.to_string());
        message
    }

    pub fn decrypt(&mut self, message: &OlmMessage) -> Result<Vec<u8>> {
        let plaintext = self.session.decrypt(message)?;
        self.their_messages.push(String::from_utf8(plaintext.clone())?);
        Ok(plaintext)
    }
}
