use crate::cryptography::{
    aes256_ctr_hmac_sha256_decrypt, aes256_ctr_hmac_sha256_encrypt, DecryptionError,
    EncryptionError,
};
use arrayref::array_ref;
use ethers_core::k256::sha2;

use rand::thread_rng;
use x25519_dalek::{PublicKey, StaticSecret};

const EPHEMERAL_KEYS_KDF_LEN: usize = 96;
const SALT_PREFIX: &[u8] = b"SealedSender";

pub struct SealedSenderMessage {
    pub ephemeral_public_key: Vec<u8>,
    pub encrypted_static_key: Vec<u8>,
    pub encrypted_message: Vec<u8>,
}

impl SealedSenderMessage {
    fn new(
        ephemeral_public_key: Vec<u8>,
        encrypted_static_key: Vec<u8>,
        encrypted_message: Vec<u8>,
    ) -> Self {
        Self {
            ephemeral_public_key,
            encrypted_static_key,
            encrypted_message,
        }
    }
}

pub(super) struct EphemeralKeys {
    pub(super) ephemeral_public_key: [u8; 32],
    pub(super) chain_key: [u8; 32],
    pub(super) cipher_key: [u8; 32],
    pub(super) mac_key: [u8; 32],
}

impl EphemeralKeys {
    pub(super) fn build(
        our_pub_key: &PublicKey,
        // TODO: Replace with EphemeralSecret once I figure out how to initialize it
        our_priv_key: &StaticSecret,
        their_pub_key: &PublicKey,
        is_sender: bool,
    ) -> Self {
        let our_pub_key_bytes = our_pub_key.as_bytes();
        let their_pub_key_bytes = their_pub_key.as_bytes();
        let ephemeral_salt = match is_sender {
            true => [SALT_PREFIX, their_pub_key_bytes, our_pub_key_bytes],
            false => [SALT_PREFIX, our_pub_key_bytes, their_pub_key_bytes],
        }
        .concat();

        let shared_secret = our_priv_key.diffie_hellman(their_pub_key);
        let mut derived_values = [0; EPHEMERAL_KEYS_KDF_LEN];
        // Generate new keys using the salt and shared secret
        hkdf::Hkdf::<sha2::Sha256>::new(Some(&ephemeral_salt), shared_secret.as_bytes())
            .expand(&[], &mut derived_values)
            .expect("valid output length");

        Self {
            ephemeral_public_key: *our_pub_key_bytes,
            // Slice the new key up into a chain key, cipher key, and mac key
            chain_key: *array_ref![&derived_values, 0, 32],
            cipher_key: *array_ref![&derived_values, 32, 32],
            mac_key: *array_ref![&derived_values, 64, 32],
        }
    }
}

pub(super) struct StaticKeys {
    pub(super) cipher_key: [u8; 32],
    pub(super) mac_key: [u8; 32],
}

impl StaticKeys {
    pub(super) fn build(
        our_priv_key: &StaticSecret,
        their_key: &PublicKey,
        chain_key: &[u8; 32],
        ctext: &[u8],
    ) -> Self {
        let salt = [chain_key, ctext].concat();

        let shared_secret = our_priv_key.diffie_hellman(their_key);
        // 96 bytes are derived, but the first 32 are discarded/unused. This is intended to
        // mirror the way the EphemeralKeys are derived, even though StaticKeys does not end up
        // requiring a third "chain key".
        let mut derived_values = [0; 96];
        hkdf::Hkdf::<sha2::Sha256>::new(Some(&salt), shared_secret.as_bytes())
            .expand(&[], &mut derived_values)
            .expect("valid output length");

        Self {
            cipher_key: *array_ref![&derived_values, 32, 32],
            mac_key: *array_ref![&derived_values, 64, 32],
        }
    }
}

pub fn sealed_sender_encrypt(
    our_pub_key: &PublicKey,
    our_private_key: &StaticSecret,
    recipient_pub_key: &PublicKey,
    message: &[u8],
) -> Result<SealedSenderMessage, EncryptionError> {
    let (ephem_priv_key, ephem_pub_key) = generate_keypair();
    // Generate the ephemeral chain key, cipher key, and mac key from the randomly generated keypair
    let e_keys = EphemeralKeys::build(&ephem_pub_key, &ephem_priv_key, recipient_pub_key, true);
    let static_key_ciphertext = aes256_ctr_hmac_sha256_encrypt(
        our_pub_key.as_bytes(),
        &e_keys.cipher_key,
        &e_keys.mac_key,
    )?;

    // These are the keys used for message encryption
    let static_keys = StaticKeys::build(
        our_private_key,
        recipient_pub_key,
        &e_keys.chain_key,
        static_key_ciphertext.as_slice(),
    );

    // Actually encrypt the message
    let encrypted_message =
        aes256_ctr_hmac_sha256_encrypt(message, &static_keys.cipher_key, &static_keys.mac_key)?;

    Ok(SealedSenderMessage::new(
        e_keys.ephemeral_public_key.to_vec(),
        static_key_ciphertext,
        encrypted_message,
    ))
}

pub fn sealed_sender_decrypt(
    our_pub_key: &PublicKey,
    our_priv_key: &StaticSecret,
    message: &SealedSenderMessage,
) -> Result<Vec<u8>, DecryptionError> {
    // Convert to [u8; 32] so it can be cast to a PublicKey
    let ephem_pub_bytes: [u8; 32] = message.ephemeral_public_key.as_slice().try_into().unwrap();
    let ephem_keys =
        EphemeralKeys::build(our_pub_key, our_priv_key, &ephem_pub_bytes.into(), false);

    // Decrypt the message key and coerce into [u8; 32]
    let message_key_bytes: [u8; 32] = aes256_ctr_hmac_sha256_decrypt(
        message.encrypted_static_key.as_slice(),
        &ephem_keys.cipher_key,
        &ephem_keys.mac_key,
    )?
    .as_slice()
    .try_into()
    .unwrap();

    let static_key: PublicKey = message_key_bytes.into();

    // Now creat the Static Keys
    let static_keys = StaticKeys::build(
        our_priv_key,
        &static_key,
        &ephem_keys.chain_key,
        &message.encrypted_static_key,
    );

    // Actually decrypt the message
    let message_bytes = aes256_ctr_hmac_sha256_decrypt(
        &message.encrypted_message,
        &static_keys.cipher_key,
        &static_keys.mac_key,
    )?;

    Ok(message_bytes)
}

fn generate_keypair() -> (StaticSecret, PublicKey) {
    let rng = thread_rng();
    let ephemeral_private_key = StaticSecret::random_from_rng(rng);
    let ephemeral_public_key = PublicKey::from(&ephemeral_private_key);

    (ephemeral_private_key, ephemeral_public_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_static_keypair() -> (StaticSecret, PublicKey) {
        let rng = thread_rng();
        let ephemeral_private_key = StaticSecret::random_from_rng(rng);
        let ephemeral_public_key = PublicKey::from(&ephemeral_private_key);

        (ephemeral_private_key, ephemeral_public_key)
    }

    #[test]
    fn test_encrypt() {
        let (my_priv_key, my_pub_key) = generate_static_keypair();
        let (_, recipient_pub_key) = generate_static_keypair();
        let message = b"hello world";

        let encrypted =
            sealed_sender_encrypt(&my_pub_key, &my_priv_key, &recipient_pub_key, message)
                .expect("encryption failed");

        assert!(encrypted.ephemeral_public_key.len() == 32);
    }

    #[test]
    fn test_round_trip() {
        let (sender_priv_key, sender_pub_key) = generate_static_keypair();
        let (recipient_priv_key, recipient_pub_key) = generate_static_keypair();
        let message = b"hello world";

        let encrypted = sealed_sender_encrypt(
            &sender_pub_key,
            &sender_priv_key,
            &recipient_pub_key,
            message,
        )
        .expect("encryption failed");

        assert!(encrypted.ephemeral_public_key.len() == 32);

        let decrypted =
            sealed_sender_decrypt(&recipient_pub_key, &recipient_priv_key, &encrypted).unwrap();

        assert_eq!(decrypted, message);
    }
}
