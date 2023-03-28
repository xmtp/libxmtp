pub mod encryption;

#[cfg(test)]
mod tests {
    use crate::*;
    use hex;

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
    fn test_simple_decryption() {
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

        let plaintext_hex = "0a88030ac00108b08b90bfe53012220a20b1d1ae465df4258351c462ea592723753a366263146c69120b4901e4c7a56c8b1a920108b08b90bfe53012440a420a401051d42da81190bbbe080f0cef3356cb476ecf87b112b22a4623f1d22ac358fa08a6160720051acf6ac651335c9114a052a7885ecfaf7c9725f9700075ac22b11a430a41046520443dc4358499e8f0269567bcc27d7264771de694eb84d5c5334e152ede227f3a1606b6dd47129d7c999a6655855cb02dc2b32ee9bf02c01578277dd4ddeb12c20108d88b90bfe53012220a20744cabc19d4d84d9753eed7091bc3047d2e46578cce75193add548f530c7f1d31a940108d88b90bfe53012460a440a409e12294d043420f762ed24e7d21f26328f0f787a964d07f7ebf288f2ab9f750b76b820339ff8cffd4be83adf7177fd29265c4479bf9ab4dc8ed9e5af399a9fab10011a430a4104e0f94416fc0431050a7f4561f8dfdd89e23d24c1d05c50710ef0524316a3bd5ed938c0f111133348fc2aeff399838ce3bd8505182e8582efc6beda0d5144330f";

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
}
