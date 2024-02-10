use base64::{engine::general_purpose, Engine as _};

use xmtp_v2::{encryption::hkdf, hashes::sha256};

const PRIVATE_PREFERENCES_TOPIC_SALT: &[u8] = b"XMTP_PRIVATE_PREFERENCES_TOPIC";

// Converts the private key into an unlinkable identifier
// Changing the salt will create a different identifier
pub fn generate_topic_identifier(secret: &[u8], salt: &[u8]) -> Result<String, String> {
    // Derive a key based on the secret and the salt
    let derived_key = hkdf(secret, salt)?;
    // Hash the derived key one more time to get a public topic identifier
    let topic = sha256(&derived_key);

    let topic = general_purpose::URL_SAFE_NO_PAD.encode(topic);
    Ok(topic)
}

pub fn generate_private_preferences_topic_identifier(secret: &[u8]) -> Result<String, String> {
    generate_topic_identifier(secret, PRIVATE_PREFERENCES_TOPIC_SALT)
}

#[cfg(test)]
mod tests {
    use crate::test::generate_keypair;

    use super::*;

    #[test]
    fn generate_topic() {
        let (priv_key, _) = generate_keypair();

        let topic = generate_topic_identifier(&priv_key.serialize(), b"test").unwrap();
        let topic_2 = generate_topic_identifier(&priv_key.serialize(), b"test-2").unwrap();

        assert!(topic != topic_2);
    }

    #[test]
    fn generate_reference_identifier() {
        // We randomly generated this key as an explicit example for reference tests across SDKs.
        let k = &[
            69, 239, 223, 17, 3, 219, 126, 21, 172, 74, 55, 18, 123, 240, 246, 149, 158, 74, 183,
            229, 236, 98, 133, 184, 95, 44, 130, 35, 138, 113, 36, 211,
        ];
        let identifier = generate_private_preferences_topic_identifier(k).unwrap();

        assert_eq!(identifier, "EBsHSM9lLmELuUVCMJ-tPE0kDcok1io9IwUO6WPC-cM");
    }
}
