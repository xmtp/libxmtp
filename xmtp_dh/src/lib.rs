uniffi_macros::include_scaffolding!("xmtp_dh");

use xmtp_crypto::k256_helper;

pub fn e2e_selftest() -> String {
    "Hello world".to_string()
}

pub fn diffie_hellman_k256(private_key_bytes: Vec<u8>, public_key_bytes: Vec<u8>) -> Vec<u8> {
    k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .unwrap()
    // .map_err(|e| format!("ECDHError: {}", e))
}
