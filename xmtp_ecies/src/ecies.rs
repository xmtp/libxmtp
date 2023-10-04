use ecies::{decrypt, encrypt};

pub fn encrypt_bytes(public_key: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    match encrypt(public_key, message) {
        Ok(ciphertext) => Ok(ciphertext),
        Err(err) => Err(format!("error encrypting bytes: {:?}", err)),
    }
}

pub fn decrypt_bytes(private_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    match decrypt(private_key, ciphertext) {
        Ok(plaintext) => Ok(plaintext),
        Err(err) => Err(format!("error decrypting bytes: {:?}", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecies::utils::generate_keypair;

    #[test]
    fn test_encrypt_decrypt() {
        let (secret_key, public_key) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();
        let ciphertext = encrypt_bytes(&public_key.serialize(), &message).unwrap();
        let plaintext = decrypt_bytes(&secret_key.serialize(), &ciphertext).unwrap();
        assert_eq!(message, plaintext);
    }
}
