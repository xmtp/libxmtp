pub mod encryption;
pub mod hashes;
pub mod k256_helper;
pub mod signature;
pub mod traits;

pub mod utils;

#[cfg(test)]
mod tests {
    use crate::*;

    // Helper function for testing
    pub fn get_personal_sign_message(message: &[u8]) -> Vec<u8> {
        // Prefix byte array is: "\x19Ethereum Signed Message:\n"
        let mut prefix = format!("\x19Ethereum Signed Message:\n{}", message.len())
            .as_bytes()
            .to_vec();
        prefix.append(&mut message.to_vec());
        prefix
    }

    #[test]
    fn test_hkdf_simple() {
        // Test Vectors generated with xmtp-js
        // Test 1
        let secret1 = hex::decode("aff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337")
            .unwrap();
        let expected1 =
            hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176")
                .unwrap();
        let derived1_result = encryption::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        assert_eq!(derived1_result.unwrap().to_vec(), expected1);

        // Test 2
        let secret2 = hex::decode("af43ad68d9fcf40967f194497246a6e30515b6c4f574ee2ff58e31df32f5f18040812188cfb5ce34e74ae27b73be08dca626b3eb55c55e6733f32a59dd1b8e021c").unwrap();
        let salt2 = hex::decode("a8500ae6f90a7ccaa096adc55857b90c03508f7d5f8d103a49d58e69058f0c3c")
            .unwrap();
        let expected2 =
            hex::decode("6181d0905f3f31cc3940336696afe1337d9e4d7f6655b9a6eaed2880be38150c")
                .unwrap();
        let derived2_result = encryption::hkdf(&secret2, &salt2);
        // Check result
        assert!(derived2_result.is_ok());
        assert_eq!(derived2_result.unwrap().to_vec(), expected2);
    }

    #[test]
    fn test_hkdf_error() {
        let secret1 = hex::decode("bff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337")
            .unwrap();
        let expected1 =
            hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176")
                .unwrap();
        let derived1_result = encryption::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        // Assert not equal
        assert_ne!(derived1_result.unwrap().to_vec(), expected1);
    }

    #[test]
    fn test_hkdf_invalid_key() {
        let secret1 = hex::decode("").unwrap();
        let salt1 = hex::decode("").unwrap();
        let derived1_result = encryption::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
    }

    #[test]
    fn test_hardcoded_decryption() {
        // Generated from xmtp-js with simple console.log statements around unit-tests that use the decrypt function
        let hkdf_salt: Vec<u8> = vec![
            139, 45, 107, 41, 87, 173, 15, 163, 250, 14, 194, 152, 200, 180, 226, 48, 140, 198, 1,
            93, 80, 253, 64, 244, 41, 69, 15, 139, 197, 77, 189, 53,
        ];
        let gcm_nonce: Vec<u8> = vec![55, 245, 104, 8, 28, 107, 41, 76, 54, 166, 179, 183];
        let payload: Vec<u8> = vec![
            29, 166, 18, 126, 14, 51, 186, 211, 216, 75, 24, 3, 137, 77, 83, 46, 162, 125, 138,
            179, 183, 125, 96, 93, 70, 57, 95, 207, 85, 199, 180, 152, 5, 238, 57, 184, 250, 185,
            32, 126, 50, 79, 154, 92, 50, 107, 120, 7, 7, 90, 19, 31, 124, 96, 88, 146, 145, 117,
            140, 25, 147, 172, 59, 30, 213, 164, 187, 53, 226, 48, 0, 147, 246, 254, 122, 194, 171,
            246, 248, 62, 62, 176, 142, 0, 230, 95, 13, 226, 215, 143, 237, 235, 105, 59, 139, 87,
            73, 176, 16, 240, 104, 7, 142, 28, 123, 226, 228, 179, 7, 255, 70, 61, 70, 5, 220, 20,
            39, 249, 110, 242, 38, 42, 14, 74, 214, 19, 232, 127, 157, 113, 149, 151, 185, 18, 149,
            23, 180, 252, 62, 31, 31, 249, 90, 38, 77, 24, 188, 38, 111, 143, 31, 137, 70, 73, 80,
            141, 145, 248, 97, 158, 53, 39, 156, 179, 135, 158, 222, 148, 117, 165, 40, 254, 210,
            66, 138, 135, 141, 159, 80, 13, 169, 236, 202, 223, 178, 185, 136, 192, 158, 237, 157,
            107, 162, 207, 111, 228, 14, 55, 48, 191, 124, 190, 201, 48, 194, 173, 82, 99, 223,
            124, 103, 30, 79, 139, 174, 234, 185, 233, 180, 91, 53, 248, 196, 188, 231, 77, 229,
            144, 9, 250, 184, 115, 146, 40, 238, 217, 135, 179, 28, 227, 31, 246, 203, 221, 104,
            140, 32, 85, 186, 59, 145, 155, 32, 92, 89, 195, 179, 36, 13, 21, 220, 75, 82, 126, 59,
            62, 187, 62, 188, 203, 5, 19, 14, 107, 66, 236, 128, 231, 185, 180, 159, 13, 70, 186,
            245, 174, 85, 209, 220, 91, 115, 76, 45, 238, 121, 141, 166, 205, 102, 86, 186, 144,
            17, 63, 221, 10, 39, 174, 189, 182, 251, 215, 222, 102, 176, 207, 251, 233, 18, 209,
            217, 226, 123, 34, 231, 124, 168, 235, 19, 248, 43, 253, 43, 58, 223, 216, 229, 156,
            70, 241, 21, 164, 151, 39, 253, 26, 16, 77, 128, 16, 237, 36, 139, 250, 192, 226, 54,
            50, 169, 181, 18, 15, 179, 133, 194, 95, 248, 231, 109, 113, 93, 241, 188, 2, 230, 83,
            79, 39, 146, 32, 151, 150, 182, 12, 7, 12, 73, 151, 191, 230, 170, 73, 249, 52, 200,
            176, 66, 98, 74, 3, 119, 227, 239, 73, 92, 80, 81, 15, 99, 185, 52,
        ];
        let secret: Vec<u8> = vec![
            124, 230, 18, 30, 212, 117, 106, 175, 141, 208, 177, 22, 206, 183, 244, 74, 178, 241,
            29, 79, 76, 175, 89, 36, 228, 189, 7, 3, 83, 115, 158, 106, 60, 139, 3, 156, 222, 117,
            237, 194, 19, 76, 127, 247, 107, 202, 93, 122, 222, 63, 229, 155, 215, 145, 243, 231,
            62, 220, 151, 225, 136, 193, 228, 82, 28,
        ];

        let plaintext_hex =
            "0a88030ac00108b08b90bfe53012220a20b1d1ae465df4258351c462ea592723753a36\
             6263146c69120b4901e4c7a56c8b1a920108b08b90bfe53012440a420a401051d42da8\
             1190bbbe080f0cef3356cb476ecf87b112b22a4623f1d22ac358fa08a6160720051acf\
             6ac651335c9114a052a7885ecfaf7c9725f9700075ac22b11a430a41046520443dc435\
             8499e8f0269567bcc27d7264771de694eb84d5c5334e152ede227f3a1606b6dd47129d\
             7c999a6655855cb02dc2b32ee9bf02c01578277dd4ddeb12c20108d88b90bfe5301222\
             0a20744cabc19d4d84d9753eed7091bc3047d2e46578cce75193add548f530c7f1d31a\
             940108d88b90bfe53012460a440a409e12294d043420f762ed24e7d21f26328f0f787a\
             964d07f7ebf288f2ab9f750b76b820339ff8cffd4be83adf7177fd29265c4479bf9ab4\
             dc8ed9e5af399a9fab10011a430a4104e0f94416fc0431050a7f4561f8dfdd89e23d24\
             c1d05c50710ef0524316a3bd5ed938c0f111133348fc2aeff399838ce3bd8505182e85\
             82efc6beda0d5144330f";

        // Invoke decrypt on the ciphertext
        let decrypt_result = encryption::decrypt(
            payload.as_slice(),
            hkdf_salt.as_slice(),
            gcm_nonce.as_slice(),
            secret.as_slice(),
            None,
        );

        assert!(decrypt_result.is_ok());
        assert_eq!(hex::encode(decrypt_result.unwrap()), plaintext_hex);
    }

    #[test]
    fn test_roundtrip_encryption_short() {
        // Simple key choice, same as previous test but I chopped a digit off the first column
        let secret: Vec<u8> = vec![
            24, 230, 18, 30, 212, 117, 106, 175, 141, 208, 177, 22, 206, 183, 244, 74, 178, 241, 9,
            79, 76, 175, 89, 36, 228, 189, 7, 3, 83, 115, 158, 106, 60, 139, 3, 156, 222, 117, 37,
            194, 19, 76, 127, 247, 107, 202, 93, 122, 222, 63, 229, 155, 215, 145, 243, 231, 2,
            220, 151, 225, 136, 193, 228, 82, 28,
        ];

        let plaintext: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let aead: Vec<u8> = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19];

        // Invoke encrypt on the plaintext
        let encrypt_result = encryption::encrypt(
            plaintext.as_slice(),
            secret.as_slice(),
            Some(aead.as_slice()),
        );

        assert!(encrypt_result.is_ok());
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

        assert!(decrypt_result.is_ok());
        assert_eq!(decrypt_result.unwrap(), plaintext);
    }

    #[test]
    fn test_roundtrip_aead_failure() {
        // Simple key choice, same as previous test but I chopped a digit off the first column
        let secret: Vec<u8> = vec![
            24, 230, 18, 30, 212, 117, 106, 175, 141, 208, 177, 22, 206, 183, 244, 74, 178, 241, 9,
            79, 76, 175, 89, 36, 228, 189, 7, 3, 83, 115, 158, 106, 60, 139, 3, 156, 222, 117, 37,
            194, 19, 76, 127, 247, 107, 202, 93, 122, 222, 63, 229, 155, 215, 145, 243, 231, 2,
            220, 151, 225, 136, 193, 228, 82, 28,
        ];

        let plaintext: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let aead: Vec<u8> = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
        // Last byte is 20 instead of 19
        let bad_aead: Vec<u8> = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 20];

        // Invoke encrypt on the plaintext
        let encrypt_result = encryption::encrypt(
            plaintext.as_slice(),
            secret.as_slice(),
            Some(aead.as_slice()),
        );

        assert!(encrypt_result.is_ok());
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
            Some(&bad_aead),
        );

        assert!(decrypt_result.is_err());
    }

    #[test]
    fn test_sha256_empty() {
        let input: Vec<u8> = vec![];
        let expected: Vec<u8> = vec![
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];

        let result = hashes::sha256(input.as_slice());
        assert_eq!(result, expected.as_slice());
    }

    #[test]
    fn test_keccak256_empty() {
        let input: Vec<u8> = vec![];
        let expected: Vec<u8> = vec![
            0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7,
            0x03, 0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04,
            0x5d, 0x85, 0xa4, 0x70,
        ];

        let result = hashes::keccak256(input.as_slice());
        assert_eq!(result, expected.as_slice());
    }

    #[test]
    fn test_sha256_abc() {
        let input: Vec<u8> = vec![0x61, 0x62, 0x63];
        let expected: Vec<u8> = vec![
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];

        let result = hashes::sha256(input.as_slice());
        assert_eq!(result, expected.as_slice());
    }

    #[test]
    fn test_keccak256_abc() {
        let input: Vec<u8> = vec![0x61, 0x62, 0x63];
        let expected: Vec<u8> = vec![
            0x4e, 0x03, 0x65, 0x7a, 0xea, 0x45, 0xa9, 0x4f, 0xc7, 0xd4, 0x7b, 0xa8, 0x26, 0xc8,
            0xd6, 0x67, 0xc0, 0xd1, 0xe6, 0xe3, 0x3a, 0x64, 0xa0, 0x36, 0xec, 0x44, 0xf5, 0x8f,
            0xa1, 0x2d, 0x6c, 0x45,
        ];

        let result = hashes::keccak256(input.as_slice());
        assert_eq!(result, expected.as_slice());
    }

    #[test]
    fn test_get_public_key() {
        let secret: Vec<u8> = vec![
            0x9d, 0x61, 0xb1, 0xde, 0x9d, 0x61, 0xb1, 0xde, 0x9d, 0x61, 0xb1, 0xde, 0x9d, 0x61,
            0xb1, 0xde, 0x9d, 0x61, 0xb1, 0xde, 0x9d, 0x61, 0xb1, 0xde, 0x9d, 0x61, 0xb1, 0xde,
            0x9d, 0x61, 0xb1, 0xde,
        ];
        let public_key = k256_helper::get_public_key(secret.as_slice()).unwrap();
        // Assert 65 bytes, first is 0x04
        assert_eq!(public_key.len(), 65);
        assert_eq!(public_key[0], 0x04);
    }

    #[test]
    fn test_public_key_from_private() {
        // Generated externally via xmtp-ios
        let identity_private_key: Vec<u8> = vec![
            0x84, 0x62, 0xd5, 0x4e, 0x18, 0x87, 0xd1, 0xb8, 0xfe, 0x75, 0x67, 0xd0, 0x6c, 0x54,
            0x60, 0xc0, 0x1c, 0x42, 0xca, 0x2b, 0x97, 0x3a, 0x3b, 0x93, 0xd4, 0xb0, 0x47, 0xc8,
            0xde, 0xfd, 0x4f, 0xda,
        ];
        let identity_public_key: Vec<u8> = vec![
            0x04, 0xd9, 0x35, 0xd2, 0xcd, 0x9d, 0x0f, 0x8d, 0x01, 0x68, 0xde, 0x02, 0x97, 0xfe,
            0xf5, 0x06, 0x0b, 0x10, 0x2b, 0x42, 0x23, 0xc7, 0x8a, 0xbc, 0x6f, 0x14, 0xbc, 0xb1,
            0x94, 0x1f, 0x05, 0xae, 0x08, 0xa9, 0xad, 0x1b, 0xfc, 0x1c, 0x07, 0x2e, 0xd7, 0x16,
            0x89, 0xf0, 0x5d, 0xdd, 0x99, 0x4b, 0xe5, 0xff, 0x41, 0xc1, 0x89, 0x7a, 0x1a, 0xc9,
            0x71, 0x81, 0x15, 0xe9, 0x4a, 0x46, 0x8a, 0xb7, 0xdc,
        ];
        // Test get_public_key from private key works
        let public_key = k256_helper::get_public_key(identity_private_key.as_slice()).unwrap();
        assert_eq!(public_key, identity_public_key);

        let pre_key_private_key: Vec<u8> = vec![
            0x9c, 0xd8, 0xc2, 0x93, 0x7a, 0xca, 0x67, 0x56, 0x5e, 0x4a, 0x96, 0x49, 0x95, 0x2b,
            0xac, 0x4e, 0x52, 0x3a, 0x21, 0x1e, 0x6e, 0x63, 0x47, 0xd3, 0xc0, 0x8f, 0x4d, 0x3a,
            0xe0, 0x96, 0xcf, 0x38,
        ];

        let pre_key_public_key: Vec<u8> = vec![
            0x04, 0xe5, 0x24, 0xb6, 0x8a, 0x0a, 0x66, 0x32, 0xf2, 0x6a, 0xb9, 0x9b, 0xa4, 0x11,
            0xe0, 0xcd, 0x99, 0x70, 0x64, 0x17, 0xd9, 0xef, 0x24, 0xf2, 0x1e, 0xc7, 0x12, 0x44,
            0x3b, 0xc0, 0xd6, 0xbd, 0x8a, 0x80, 0x12, 0x9f, 0xcd, 0x47, 0x7d, 0x46, 0x1e, 0x6d,
            0x18, 0x25, 0xc4, 0x00, 0xa1, 0xc6, 0x5c, 0xc0, 0x1a, 0x06, 0xc6, 0x6c, 0xa0, 0x0f,
            0x9c, 0x27, 0x85, 0xdf, 0x41, 0x0e, 0xa9, 0xda, 0x82,
        ];

        let public_key = k256_helper::get_public_key(pre_key_private_key.as_slice()).unwrap();
        assert_eq!(public_key, pre_key_public_key);
    }

    #[test]
    fn test_public_key_recovery_sha256() {
        // message: 48656c6c6f20776f726c64 // "hello world"
        // Signature: 8268b9c8d7629e9bbbd392004c337a5d7c99d1e59cf26a4a4f024935462cbf277f34e41be8fbcc9a5ffe69adcd0461e04b740df4180ce4864222236d735234a301
        // Expected public: 04fd927bcc71326a7a294f202133d94f4064f38ede547bc9449ed7307eecc23a845214a6c2880d64a352ec8410f90f5263fb00d6536aa27c7441d4fd6b97f4e518
        let message = hex::decode("48656c6c6f20776f726c64").unwrap();
        let signature = hex::decode("8268b9c8d7629e9bbbd392004c337a5d7c99d1e59cf26a4a4f024935462cbf277f34e41be8fbcc9a5ffe69adcd0461e04b740df4180ce4864222236d735234a301").unwrap();
        let expected_public_bytes = hex::decode("04fd927bcc71326a7a294f202133d94f4064f38ede547bc9449ed7307eecc23a845214a6c2880d64a352ec8410f90f5263fb00d6536aa27c7441d4fd6b97f4e518").unwrap();

        let public_key =
            k256_helper::recover_public_key_predigest_sha256(&message, &signature).unwrap();
        assert_eq!(public_key, expected_public_bytes);
    }

    #[test]
    fn test_public_key_recovery_keccak256() {
        // Best way to test recovery with keccak256 is to test our existing XMTP Create Identity Signature
        // pre eth personal sign message:  584d5450203a20437265617465204964656e746974790a30383830633662626331663638396565616231373161343330613431303431653361663730396132666136633439336664623434653332396362656561303464303966316666653561643465303133623866656230313836633232613532646331313836643462353835626465393139313238323039396531366663366565643830303333346138626334353363366561303734363634616138656465630a0a466f72206d6f726520696e666f3a2068747470733a2f2f786d74702e6f72672f7369676e6174757265732f
        // wallet address:  '0x13d0b11D0157F1740e139171C98FFF95b83AD107'
        // identity key signature d5ea9e13675a39295d82b0ee9fe2782b0754f93666e280e75451e84306953aac5979d039126d5e516ebb4b5150b8d128b7e3b71f9dce1d4939a2456c52fbf3fc
        // identity key recovery 0
        // expected public key bytes (64, no 0x04 prefix): 96fdd6c4e8c00e02642e800fe965808eb89e4f256fc913159950995a142289fad97bfd119e2afe5ee9de765e26f09e6f86e300616a8829228f57a6cc47e42381
        let message = hex::decode("584d5450203a20437265617465204964656e746974790a30383830633662626331663638396565616231373161343330613431303431653361663730396132666136633439336664623434653332396362656561303464303966316666653561643465303133623866656230313836633232613532646331313836643462353835626465393139313238323039396531366663366565643830303333346138626334353363366561303734363634616138656465630a0a466f72206d6f726520696e666f3a2068747470733a2f2f786d74702e6f72672f7369676e6174757265732f").unwrap();
        let signature = hex::decode("d5ea9e13675a39295d82b0ee9fe2782b0754f93666e280e75451e84306953aac5979d039126d5e516ebb4b5150b8d128b7e3b71f9dce1d4939a2456c52fbf3fc00").unwrap();
        let expected_public_bytes = hex::decode("0496fdd6c4e8c00e02642e800fe965808eb89e4f256fc913159950995a142289fad97bfd119e2afe5ee9de765e26f09e6f86e300616a8829228f57a6cc47e42381").unwrap();

        // Need to do EIP-191 style message preparation for personal signature
        let ethmessage = get_personal_sign_message(&message);
        let public_key =
            k256_helper::recover_public_key_predigest_keccak256(&ethmessage, &signature).unwrap();
        assert_eq!(public_key, expected_public_bytes);
    }
}
