use crate::{hash::sha3_256, hkdf::hkdf};
use base64::{engine::general_purpose, Engine as _};

const PRIVATE_PREFERENCES_TOPIC_SALT: &[u8] = b"XMTP_PRIVATE_PREFERENCES_TOPIC";

// Converts the private key into an unlinkable identifier
// Changing the salt will create a different identifier
pub fn generate_topic_identifier(secret: &[u8], salt: &[u8]) -> Result<String, String> {
    // Derive a key based on the secret and the salt
    let derived_key = hkdf(secret, salt)?;
    // Hash the derived key one more time to get a public topic identifier
    let topic = sha3_256(&derived_key);

    let topic = general_purpose::URL_SAFE_NO_PAD.encode(topic);
    Ok(topic)
}

pub fn generate_private_preferences_topic_identifier(secret: &[u8]) -> Result<String, String> {
    generate_topic_identifier(secret, PRIVATE_PREFERENCES_TOPIC_SALT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecies::utils::generate_keypair;

    #[test]
    fn generate_topic() {
        let (priv_key, _) = generate_keypair();

        let topic = generate_topic_identifier(&priv_key.serialize(), b"test").unwrap();
        let topic_2 = generate_topic_identifier(&priv_key.serialize(), b"test-2").unwrap();

        assert!(topic != topic_2);
    }
}
