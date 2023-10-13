use crate::{hash::sha3_256, hkdf::hkdf};
use base64::{engine::general_purpose, Engine as _};

pub fn generate_hkdf_topic(secret: &[u8], salt: &[u8]) -> Result<String, String> {
    // Derive a key based on the secret and the salt
    let derived_key = hkdf(secret, salt)?;
    // Hash the derived key one more time to get a public topic identifier
    let topic = sha3_256(&derived_key);

    let topic = general_purpose::URL_SAFE_NO_PAD.encode(topic);
    Ok(topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecies::utils::generate_keypair;

    #[test]
    fn generate_topic() {
        let (priv_key, _) = generate_keypair();

        let topic = generate_hkdf_topic(&priv_key.serialize(), b"test").unwrap();
        let topic_2 = generate_hkdf_topic(&priv_key.serialize(), b"test-2").unwrap();

        assert!(topic != topic_2);
    }
}
