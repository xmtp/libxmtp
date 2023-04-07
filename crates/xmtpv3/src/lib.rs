pub mod account;
pub mod manager;
pub mod session;

#[cfg(test)]
mod tests {
    use crate::manager::{e2e_selftest, VoodooInstance};

    #[test]
    pub fn test_e2e() {
        let selftest_result = e2e_selftest().unwrap();
        assert_eq!(selftest_result, "Self test successful");
    }

    #[test]
    pub fn test_simple_conversation() {
        let mut alice = VoodooInstance::new();
        let mut bob = VoodooInstance::new();
        let bob_public = bob.public_account();
        let alice_public = alice.public_account();

        let (alice_session_id, alice_msg) = alice
            .create_outbound_session(&bob_public, "Hello Bob")
            .unwrap();

        let (bob_session_id, bob_plaintext) = bob
            .create_inbound_session(&alice_public, &alice_msg)
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
        let bob_public = bob.public_account();
        let alice_public = alice.public_account();

        let (alice_session_id, alice_msg) = alice
            .create_outbound_session_serialized(&bob_public, "Hello Bob")
            .unwrap();

        let (bob_session_id, bob_plaintext) = bob
            .create_inbound_session_serialized(&alice_public, &alice_msg)
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
    pub fn test_conversation_and_message_counts() {
        let mut alice = VoodooInstance::new();
        let mut bob = VoodooInstance::new();
        let bob_public = bob.public_account();
        let alice_public = alice.public_account();

        let (alice_session_id, alice_msg) = alice
            .create_outbound_session(&bob_public, "Hello Bob")
            .unwrap();

        let (bob_session_id, bob_plaintext) = bob
            .create_inbound_session(&alice_public, &alice_msg)
            .unwrap();

        assert_eq!(alice_session_id, bob_session_id);
        assert_eq!(bob_plaintext, "Hello Bob");

        let bob_msg = bob.encrypt_message(&bob_session_id, "Hello Alice").unwrap();

        let alice_plaintext = alice.decrypt_message(&alice_session_id, &bob_msg).unwrap();

        assert_eq!(alice_plaintext, "Hello Alice");

        // another round, this time lots of bob messages
        let bob_msg2 = bob.encrypt_message(&bob_session_id, "2").unwrap();
        let bob_msg3 = bob.encrypt_message(&bob_session_id, "3").unwrap();

        // Check that the session bob has with alice has 3 sent, 1 received
        let bob_session = bob.sessions.get(&bob_session_id).unwrap();
        assert_eq!(bob_session.my_messages.len(), 3);
        assert_eq!(bob_session.their_messages.len(), 0);
        
        let alice_session = alice.sessions.get(&alice_session_id).unwrap();
        assert_eq!(alice_session.my_messages.len(), 0);
        // Will not be 3 because two messages have not been fed into Alice's decrypt
        assert_eq!(alice_session.their_messages.len(), 1);

        let _ = alice.decrypt_message(&alice_session_id, &bob_msg2).unwrap();
        let _ = alice.decrypt_message(&alice_session_id, &bob_msg3).unwrap();
        let alice_session = alice.sessions.get(&alice_session_id).unwrap();
        assert_eq!(alice_session.their_messages.len(), 3);

        // Check bob's last message against Alice's message store
        assert_eq!(alice_session.their_messages[2], "3");
        assert_eq!(bob_session.my_messages[2], "3");
    }
}
