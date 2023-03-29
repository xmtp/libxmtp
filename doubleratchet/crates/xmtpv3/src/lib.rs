#[cfg(test)]
mod tests {
    use anyhow::Result;
    use vodozemac::olm::{Account, AccountPickle, OlmMessage, SessionConfig};

    #[test]
    pub fn test_e2e() -> Result<()> {
        let alice = Account::new();
        let mut bob = Account::new();

        bob.generate_one_time_keys(1);
        let bob_otk = *bob.one_time_keys().values().next().unwrap();

        let mut alice_session = alice.create_outbound_session(
            SessionConfig::version_2(),
            bob.curve25519_key(),
            bob_otk,
        );

        bob.mark_keys_as_published();

        let message = "Keep it between us, OK?";
        let alice_msg = alice_session.encrypt(message);

        if let OlmMessage::PreKey(m) = alice_msg.clone() {
            let result = bob.create_inbound_session(alice.curve25519_key(), &m)?;

            let mut bob_session = result.session;
            let what_bob_received = result.plaintext;

            assert_eq!(alice_session.session_id(), bob_session.session_id());

            assert_eq!(message.as_bytes(), what_bob_received);

            let bob_reply = "Yes. Take this, it's dangerous out there!";
            let bob_encrypted_reply = bob_session.encrypt(bob_reply).into();

            let what_alice_received = alice_session.decrypt(&bob_encrypted_reply)?;
            assert_eq!(what_alice_received, bob_reply.as_bytes());
        }

        Ok(())
    }

    #[test]
    pub fn test_pickle_account_roundtrip() -> Result<()> {
        const PICKLE_KEY: [u8; 32] = [0u8; 32];
        let mut account = Account::new();

        account.generate_one_time_keys(10);
        account.generate_fallback_key();

        let pickle = account.pickle().encrypt(&PICKLE_KEY);

        let account2: Account = AccountPickle::from_encrypted(&pickle, &PICKLE_KEY)?.into();

        assert_eq!(account.identity_keys(), account2.identity_keys());

        Ok(())
    }
}
