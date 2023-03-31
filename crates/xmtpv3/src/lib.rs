use anyhow::Result;

use serde_json::json;
use std::collections::HashMap;
use vodozemac::olm::{Account, OlmMessage, Session, SessionConfig};

// This struct contains all logic for a Voodoo messaging session
pub struct VoodooInstance {
    pub account: Account,
    pub sessions: HashMap<String, Session>,
}

impl Default for VoodooInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl VoodooInstance {
    // Create a new VoodooInstance
    pub fn new() -> Self {
        let mut account = Account::new();
        // TODO: how many one-time keys should we generate?
        account.generate_one_time_keys(10);
        account.generate_fallback_key();
        Self {
            account,
            sessions: HashMap::new(),
        }
    }

    pub fn pickle_account(&self) -> String {
        // TODO: hardcoded pickle key for now just for testing
        const PICKLE_KEY: [u8; 32] = [0u8; 32];
        self.account.pickle().encrypt(&PICKLE_KEY)
    }

    // Creates an outbound session and returns a handle which is just the index
    // TODO: STOPSHIP: this function for testing assumes access to the recipient's account instance
    // this will not happen in practice
    pub fn create_outbound_session(
        &mut self,
        other_account: &mut Account,
        message: &str,
    ) -> Result<(String, OlmMessage)> {
        let other_otk = *other_account
            .one_time_keys()
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No remaining OTKs"))?;
        let mut session = self.account.create_outbound_session(
            SessionConfig::version_2(),
            other_account.curve25519_key(),
            other_otk,
        );
        let ciphertext = session.encrypt(message);
        other_account.mark_keys_as_published();
        let session_id = session.session_id();
        self.sessions.insert(session_id.clone(), session);
        Ok((session_id, ciphertext))
    }

    // Wrapper that serializes the output message
    pub fn create_outbound_session_serialized(
        &mut self,
        other_account: &mut Account,
        message: &str,
    ) -> Result<(String, String)> {
        // Call self.create_outbound_session, but then serialize the OlmMessage response with json!
        let (session_id, ciphertext) = self.create_outbound_session(other_account, message)?;
        let ciphertext_json = json!(ciphertext);
        Ok((session_id, ciphertext_json.to_string()))
    }

    // Receive incoming session start message
    // TODO: STOPSHIP: this function for testing assumes access to the sender's account instance
    pub fn create_inbound_session(
        &mut self,
        other_account: &mut Account,
        message: &OlmMessage,
    ) -> Result<(String, String)> {
        if let OlmMessage::PreKey(m) = message {
            let result = self
                .account
                .create_inbound_session(other_account.curve25519_key(), m)?;

            let self_session = result.session;
            let received_plaintext = result.plaintext;
            // Check if we have the session already
            let session_id = self_session.session_id();
            match self.sessions.get(&session_id) {
                Some(_) => {
                    // We already have the session, so we can just return the plaintext
                }
                None => {
                    // We don't have the session, so we need to store it and return the plaintext
                    self.sessions.insert(session_id.clone(), self_session);
                }
            }
            // Decode received_plaintext bytes as utf-8
            let utf8_decoded_plaintext = String::from_utf8(received_plaintext)?;
            Ok((session_id, utf8_decoded_plaintext))
        } else {
            Err(anyhow::anyhow!("Invalid message type"))
        }
    }

    // Wrapper that receives the input message and deserializes it first
    pub fn create_inbound_session_serialized(
        &mut self,
        other_account: &mut Account,
        message: &str,
    ) -> Result<(String, String)> {
        let message: OlmMessage = serde_json::from_value(serde_json::from_str(message)?)?;
        self.create_inbound_session(other_account, &message)
    }

    // Encrypts a message for a given session
    pub fn encrypt_message(&mut self, session_id: &str, message: &str) -> Result<OlmMessage> {
        let session = self.sessions.get_mut(session_id).unwrap();
        let ciphertext = session.encrypt(message);
        Ok(ciphertext)
    }

    // Decrypts a message for a given session
    pub fn decrypt_message(&mut self, session_id: &str, message: &OlmMessage) -> Result<String> {
        let session = self.sessions.get_mut(session_id).unwrap();
        let plaintext = session.decrypt(message)?;
        let utf8_decoded_plaintext = String::from_utf8(plaintext)?;
        Ok(utf8_decoded_plaintext)
    }

    // Serialized helpers
    pub fn encrypt_message_serialized(
        &mut self,
        session_id: &str,
        message: &str,
    ) -> Result<String> {
        let ciphertext = self.encrypt_message(session_id, message)?;
        let ciphertext_json = json!(ciphertext);
        Ok(ciphertext_json.to_string())
    }

    pub fn decrypt_message_serialized(
        &mut self,
        session_id: &str,
        ciphertext: &str,
    ) -> Result<String> {
        let ciphertext_message: OlmMessage =
            serde_json::from_value(serde_json::from_str(ciphertext)?)?;
        self.decrypt_message(session_id, &ciphertext_message)
    }
}

// Meant to be used by library consumers to check if the library is working
pub fn e2e_selftest() -> Result<String> {
    console_error_panic_hook::set_once();
    let alice = Account::new();
    let mut bob = Account::new();

    bob.generate_one_time_keys(1);
    let bob_otk = *bob.one_time_keys().values().next().unwrap();

    let mut alice_session =
        alice.create_outbound_session(SessionConfig::version_2(), bob.curve25519_key(), bob_otk);

    bob.mark_keys_as_published();

    let message = "Keep it between us, OK?";
    let alice_msg = alice_session.encrypt(message);

    if let OlmMessage::PreKey(m) = alice_msg {
        let result = bob.create_inbound_session(alice.curve25519_key(), &m)?;

        let mut bob_session = result.session;
        let what_bob_received = result.plaintext;

        if what_bob_received != message.as_bytes() {
            return Err(anyhow::anyhow!("what_bob_received != message.as_bytes()"));
        }

        if alice_session.session_id() != bob_session.session_id() {
            return Err(anyhow::anyhow!(
                "alice_session.session_id() != bob_session.session_id()"
            ));
        }

        let bob_reply = "Yes. Take this, it's dangerous out there!";
        let bob_encrypted_reply = bob_session.encrypt(bob_reply);

        let what_alice_received = alice_session.decrypt(&bob_encrypted_reply)?;
        if what_alice_received != bob_reply.as_bytes() {
            // Return error
            return Err(anyhow::anyhow!(
                "what_alice_received != bob_reply.as_bytes()"
            ));
        }
    }

    Ok("Self test successful".to_string())
}

#[cfg(test)]
mod tests {
    use crate::*;
    use vodozemac::olm::{Account, AccountPickle};

    #[test]
    pub fn test_e2e() {
        let selftest_result = e2e_selftest().unwrap();
        assert_eq!(selftest_result, "Self test successful");
    }

    #[test]
    pub fn test_simple_conversation() {
        let mut alice = VoodooInstance::new();
        let mut bob = VoodooInstance::new();

        let (alice_session_id, alice_msg) = alice
            .create_outbound_session(&mut bob.account, "Hello Bob")
            .unwrap();

        let (bob_session_id, bob_plaintext) = bob
            .create_inbound_session(&mut alice.account, &alice_msg)
            .unwrap();

        assert_eq!(alice_session_id, bob_session_id);
        assert_eq!(bob_plaintext, "Hello Bob");

        let bob_msg = bob.encrypt_message(&bob_session_id, "Hello Alice").unwrap();

        let alice_plaintext = alice.decrypt_message(&alice_session_id, &bob_msg).unwrap();

        assert_eq!(alice_plaintext, "Hello Alice");
    }

    #[test]
    pub fn test_serialized_conversation() {
        let mut alice = VoodooInstance::new();
        let mut bob = VoodooInstance::new();

        let (alice_session_id, alice_msg) = alice
            .create_outbound_session_serialized(&mut bob.account, "Hello Bob")
            .unwrap();

        let (bob_session_id, bob_plaintext) = bob
            .create_inbound_session_serialized(&mut alice.account, &alice_msg)
            .unwrap();

        // Assert that alice_msg is valid JSON string
        let _alice_msg_json: serde_json::Value = serde_json::from_str(&alice_msg).unwrap();
        assert_eq!(alice_session_id, bob_session_id);
        assert_eq!(bob_plaintext, "Hello Bob");

        let bob_msg = bob
            .encrypt_message_serialized(&bob_session_id, "Hello Alice")
            .unwrap();

        let alice_plaintext = alice
            .decrypt_message_serialized(&alice_session_id, &bob_msg)
            .unwrap();

        assert_eq!(alice_plaintext, "Hello Alice");
    }

    #[test]
    pub fn test_pickle_account_roundtrip() {
        const PICKLE_KEY: [u8; 32] = [0u8; 32];
        let voodoo_instance = VoodooInstance::new();

        let pickle = voodoo_instance.pickle_account();

        let account2: Account = AccountPickle::from_encrypted(&pickle, &PICKLE_KEY)
            .unwrap()
            .into();

        assert_eq!(
            voodoo_instance.account.identity_keys(),
            account2.identity_keys()
        );
    }
}
