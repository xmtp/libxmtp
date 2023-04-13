use corecrypto::encryption;


#[no_mangle]
pub extern "C" fn encryption_selftest() -> bool {
    // Simple key choice, same as previous test but I chopped a digit off the first column
    let secret: Vec<u8> = vec![
        24, 230, 18, 30, 212, 117, 106, 175, 141, 208, 177, 22, 206, 183, 244, 74, 178, 241, 9, 79,
        76, 175, 89, 36, 228, 189, 7, 3, 83, 115, 158, 106, 60, 139, 3, 156, 222, 117, 37, 194, 19,
        76, 127, 247, 107, 202, 93, 122, 222, 63, 229, 155, 215, 145, 243, 231, 2, 220, 151, 225,
        136, 193, 228, 82, 28,
    ];

    let plaintext: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let aead: Vec<u8> = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19];

    // Invoke encrypt on the plaintext
    let encrypt_result = encryption::encrypt(
        plaintext.as_slice(),
        secret.as_slice(),
        Some(aead.as_slice()),
    );

    if encrypt_result.is_err() {
        return false;
    }
    let encryption::Ciphertext {
        payload,
        hkdf_salt,
        gcm_nonce,
    } = encrypt_result.unwrap();

    // Invoke decrypt on the ciphertext
    let decrypt_result = encryption::decrypt(
        payload.as_slice(),
        hkdf_salt.as_slice(),
        gcm_nonce.as_slice(),
        secret.as_slice(),
        Some(&aead),
    );

    if decrypt_result.is_err() {
        return false;
    }
    if decrypt_result.unwrap() != plaintext {
        return false;
    }
    true
}

#[no_mangle]
pub extern "C" fn networking_selftest() -> u16 {
    xmtp_networking::selftest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption() {
        assert!(encryption_selftest());
    }

    #[test]
    fn test_networking() {
        let status_code = networking_selftest();
        assert_eq!(status_code, 200);
    }
}
