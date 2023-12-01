mod encryption;
pub mod topic;

use prost::Message as ProstMessage;
use xmtp_proto::xmtp::message_contents::{
    private_preferences_payload::Version as PrivatePreferencesVersion, Ciphertext,
    PrivatePreferencesPayload,
};

use crate::encryption::{decrypt_ciphertext, encrypt_to_ciphertext};

pub fn encrypt_message(
    private_key: &[u8], // secp256k1 private key
    message: &[u8],     // any byte array
    topic: String,      // the topic you are encrypting for
) -> Result<Vec<u8>, String> {
    let additional_data = topic.as_bytes();
    let ciphertext = encrypt_to_ciphertext(private_key, message, additional_data)?;
    let pppp_message = PrivatePreferencesPayload {
        version: Some(PrivatePreferencesVersion::V1(ciphertext)),
    };

    Ok(pppp_message.encode_to_vec())
}

pub fn decrypt_message(
    private_key: &[u8], // secp256k1 private key
    message: &[u8], // message encrypted with `encrypt_message`. Should be an encoded PrivatePreferencesPayload
    topic: String,  // the topic you are decrypting from
) -> Result<Vec<u8>, String> {
    let additional_data = topic.as_bytes();
    let ciphertext = get_ciphertext(message)?;
    let payload_bytes = decrypt_ciphertext(private_key, ciphertext, additional_data)?;

    Ok(payload_bytes)
}

fn get_ciphertext(message: &[u8]) -> Result<Ciphertext, String> {
    let ecies_message =
        PrivatePreferencesPayload::decode(message).map_err(|e| format!("{:?}", e))?;
    let ciphertext = match ecies_message.version {
        Some(PrivatePreferencesVersion::V1(ciphertext)) => ciphertext,
        None => return Err("no ciphertext found".to_string()),
    };

    Ok(ciphertext)
}

#[cfg(test)]
mod test {
    use super::*;
    use libsecp256k1::{PublicKey, SecretKey};

    pub fn generate_keypair() -> (SecretKey, PublicKey) {
        let secret_key = SecretKey::random(&mut rand::thread_rng());
        let public_key = PublicKey::from_secret_key(&secret_key);

        (secret_key, public_key)
    }

    #[test]
    fn test_round_trip() {
        let (private_key, _) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();
        let topic = "/xmtp/0/foo".to_string();

        let encrypted = encrypt_message(&private_key.serialize(), &message, topic.clone()).unwrap();
        assert!(encrypted.len() > 0);

        let decrypted = decrypt_message(&private_key.serialize(), &encrypted, topic).unwrap();

        assert_eq!(message, decrypted);
    }

    #[test]
    fn decrypt_fails_with_incorrect_private_key() {
        let (private_key, _) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();
        let topic = "/xmtp/0/foo".to_string();

        let encrypted = encrypt_message(&private_key.serialize(), &message, topic.clone()).unwrap();
        assert!(encrypted.len() > 0);
        let (other_private_key, _) = generate_keypair();

        let decrypt_result = decrypt_message(&other_private_key.serialize(), &encrypted, topic);

        assert_eq!(decrypt_result.is_err(), true);
    }

    #[test]
    fn decrypt_fails_with_incorrect_topic() {
        let (private_key, _) = generate_keypair();
        let message = "hi".as_bytes().to_vec();
        let topic = "/xmtp/0/foo".to_string();

        let encrypted = encrypt_message(&private_key.serialize(), &message, topic.clone()).unwrap();

        let ciphertext = get_ciphertext(&encrypted).unwrap();
        let failed_decrypt =
            decrypt_ciphertext(&private_key.serialize(), ciphertext, "bar".as_bytes());
        assert!(failed_decrypt.is_err());
    }
}
