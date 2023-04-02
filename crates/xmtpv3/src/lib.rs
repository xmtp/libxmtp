mod account;
mod manager;

use manager::{e2e_selftest, VoodooInstance};

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
}
