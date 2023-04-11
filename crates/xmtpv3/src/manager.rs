use anyhow::Result;

use serde_json::json;
use std::collections::HashMap;
use vodozemac::{
    olm::{Account, OlmMessage, SessionConfig},
    Curve25519PublicKey,
};

use crate::account::{VoodooContactBundlePickle, VoodooPublicIdentity};
use crate::session::VoodooSession;

// This struct contains all logic for a Voodoo messaging session
pub struct VoodooInstance {
    pub account: Account,
    pub sessions: HashMap<String, VoodooSession>,
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

    // TODO: STARTINGTASK: (see account.rs and improve this function)
    // This function should provide a pickled version of public parts of the account
    // This is used for sharing the account with other users
    pub fn public_account(&self) -> VoodooPublicIdentity {
        VoodooPublicIdentity::new(&self.account)
    }

    pub fn public_account_json(&self) -> Result<String> {
        let public_account = self.public_account();
        serde_json::to_string(&public_account).map_err(|e| e.into())
    }

    // TODO: STARTINGTASK: The goal is to remove this pattern altogether and
    // build a better abstraction for the public account, rather than creating
    // new VoodooInstances for all contacts
    pub fn from_public_account_json(public_account_json: &str) -> Result<Self> {
        let public_account: VoodooPublicIdentity = serde_json::from_str(public_account_json)?;
        let account = public_account.get_account()?;
        Ok(Self {
            account,
            sessions: HashMap::new(),
        })
    }

    pub fn identity_key(&self) -> Curve25519PublicKey {
        self.account.curve25519_key()
    }

    // TODO: For now we serve the contact bundle from the client, but eventually
    // we want to upload a public bundle to the server containing a batch of prekeys,
    // and have the server rotate through the prekeys with each contact bundle
    // request
    pub fn next_contact_bundle(&self) -> VoodooContactBundlePickle {
        VoodooContactBundlePickle::new(&self.account)
    }

    pub fn next_contact_bundle_json(&self) -> Result<String> {
        let public_account = self.public_account();
        serde_json::to_string(&public_account).map_err(|e| e.into())
    }

    // Creates an outbound session and returns a handle which is just the index
    // // TODO: STARTINGTASK: this should take the one-time-keys and pre-keys as
    // arguments too, part of the VoodooPublicIdentity maybe?
    pub fn create_outbound_session(
        &mut self,
        contact_bundle: &VoodooContactBundlePickle,
        message: &str,
    ) -> Result<(String, OlmMessage)> {
        let mut session = self.account.create_outbound_session(
            SessionConfig::version_2(),
            contact_bundle.identity_key(),
            contact_bundle.one_time_key(),
        );
        let ciphertext = session.encrypt(message);
        let session_id = session.session_id();
        self.sessions
            .insert(session_id.clone(), VoodooSession::new(session));
        Ok((session_id, ciphertext))
    }

    // Wrapper that serializes the output message
    pub fn create_outbound_session_serialized(
        &mut self,
        contact_bundle: &VoodooContactBundlePickle,
        message: &str,
    ) -> Result<(String, String)> {
        // Call self.create_outbound_session, but then serialize the OlmMessage response with json!
        let (session_id, ciphertext) = self.create_outbound_session(contact_bundle, message)?;
        let ciphertext_json = json!(ciphertext);
        Ok((session_id, ciphertext_json.to_string()))
    }

    // Receive incoming session start message
    // TODO: STARTINGTASK: how can we consolidate this so receiving a message doesn't require
    // knowing it's a PreMessage etc?
    pub fn create_inbound_session(
        &mut self,
        their_identity_key: Curve25519PublicKey,
        message: &OlmMessage,
    ) -> Result<(String, String)> {
        if let OlmMessage::PreKey(m) = message {
            let result = self.account.create_inbound_session(their_identity_key, m)?;

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
                    self.sessions
                        .insert(session_id.clone(), VoodooSession::new(self_session));
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
        their_identity_key: Curve25519PublicKey,
        message: &str,
    ) -> Result<(String, String)> {
        let message: OlmMessage = serde_json::from_value(serde_json::from_str(message)?)?;
        self.create_inbound_session(their_identity_key, &message)
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
