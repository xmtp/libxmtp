// Import k256 crate
use k256::{
    ecdsa::{SigningKey, Signature, RecoveryId, VerifyingKey, signature::{Verifier}},
    EncodedPoint,
    PublicKey,
    SecretKey,
};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use sha2::{Sha256, Digest};
use sha3::{Keccak256};

use protobuf;
use super::proto;

pub struct EcPrivateKey {
    private_key: SecretKey,
    public_key: PublicKey,
}

impl EcPrivateKey {

    // Static function to parse an EcPrivateKey from proto::private_key::PrivateKeyBundleV2
    pub fn from_proto(private_key_bundle: &proto::private_key::PrivateKeyBundleV2) -> Result<EcPrivateKey, String> {
        // Check if secp256k1 is available
        if !private_key_bundle.identity_key.has_secp256k1() {
            println!("No secp256k1 key found");
        }

        // Parse the private key from the proto
        let secret_key_bytes = private_key_bundle.identity_key.secp256k1().bytes.as_slice();
        // Check that bytes are not empty
        if secret_key_bytes.is_empty() {
            return Err("No bytes found".to_string());
        }

        // Try to derive secret key from big-endian hex-encoded BigInt, check the result
        let secret_key_result = SecretKey::from_be_bytes(secret_key_bytes);
        if secret_key_result.is_err() {
            return Err(secret_key_result.err().unwrap().to_string());
        }
        let secret_key = secret_key_result.unwrap();
        let public_key = secret_key.public_key().clone();
        return Ok(EcPrivateKey {
            private_key: secret_key,
            public_key: public_key,
        });
    }

    pub fn eth_wallet_address_from_public_key(public_key_bytes: &[u8]) -> Result<String, String> {
        // Hash the public key bytes
        let mut hasher = Keccak256::new();
        hasher.update(public_key_bytes);
        let result = hasher.finalize();
        // Return the result as hex string, take the last 20 bytes
        return Ok(format!("0x{}", hex::encode(&result[12..])));
    }

    pub fn eth_address(&self) -> Result<String, String> {
        // Get the public key bytes
        let binding = self.public_key.to_encoded_point(false);
        let public_key_bytes = binding.as_bytes();
        println!("Public key bytes: {}", public_key_bytes.len());
        println!("Hex encoded public key bytes: {}", hex::encode(public_key_bytes));
        // Return the result as hex string, take the last 20 bytes
        // Need to remove the 04 prefix for uncompressed point representation
        return EcPrivateKey::eth_wallet_address_from_public_key(&public_key_bytes[1..]);
    }

    pub fn xmtp_identity_key_payload(public_key_bytes: &[u8]) -> Vec<u8> {
        let raw_string = format!("XMTP : Create Identity\n{}\n\nFor more info: https://xmtp.org/signatures/", hex::encode(public_key_bytes));
        // Return the string utf-8 encoded
        return raw_string.as_bytes().to_vec();
    }

    // https://github.com/ethereumjs/ethereumjs-util/blob/ebf40a0fba8b00ba9acae58405bca4415e383a0d/src/signature.ts#L168
    pub fn ethereum_personal_sign_payload(xmtp_payload: &[u8]) -> Vec<u8> {
        // Prefix byte array is: "\x19Ethereum Signed Message:\n32"
        let mut prefix = format!("\x19Ethereum Signed Message:\n{}", xmtp_payload.len()).as_bytes().to_vec();
        prefix.append(&mut xmtp_payload.to_vec());
        return prefix;
//
//        // Hash the entire thing one more time with keccak256
//        let mut hasher = Keccak256::new();
//        hasher.update(prefix);
//        let result = hasher.finalize();
//        println!("Ethereum personal sign payload: {}", hex::encode(&result));
//        return result.to_vec();
    }

    pub fn ethereum_personal_digest(xmtp_payload: &[u8]) -> Vec<u8> {
        // Hash the entire thing one more time with keccak256
        let personal_sign_payload = EcPrivateKey::ethereum_personal_sign_payload(xmtp_payload);
        let mut hasher = Keccak256::new();
        hasher.update(xmtp_payload);
        let result = hasher.finalize();
        println!("Ethereum personal digest: {}", hex::encode(&result));
        return result.to_vec();
    }

    // Verify wallet signature from proto
    pub fn verify_wallet_signature(address: &str, message: &[u8], signature: &proto::signature::Signature, recid: u8) -> Result<(), String> {
        // Expect ecdsa_compact field with subfields: bytes, recovery_id
        if !signature.has_wallet_ecdsa_compact() {
            return Err("No wallet_ecdsa_compact field found".to_string());
        }
        let wallet_ecdsa_compact = signature.wallet_ecdsa_compact();
        let signature_bytes = wallet_ecdsa_compact.bytes.as_slice();
        println!("Signature bytes: {}", hex::encode(&signature_bytes));
        println!("recover: {}", wallet_ecdsa_compact.recovery);
        let recovery_id_result = RecoveryId::try_from(recid);
        if recovery_id_result.is_err() {
            return Err(recovery_id_result.err().unwrap().to_string());
        }
        let recovery_id = recovery_id_result.unwrap();
        println!("Len of signature bytes: {}", signature_bytes.len());
        let ecdsa_signature_result = Signature::try_from(signature_bytes);
        if ecdsa_signature_result.is_err() {
            return Err(ecdsa_signature_result.err().unwrap().to_string());
        }
        let ec_signature = ecdsa_signature_result.unwrap();

        let recovered_key_result = VerifyingKey::recover_from_digest(
            Keccak256::new_with_prefix(message),
            &ec_signature,
            recovery_id,
        );
        if recovered_key_result.is_err() {
            return Err(recovered_key_result.err().unwrap().to_string());
        }
        let recovered_key = recovered_key_result.unwrap();
        // Check if ethereum address from recovered key matches the address from the proto
        // First extract the public key from the recovered key
        let public_key = recovered_key.to_encoded_point(true);
        let public_key_bytes = public_key.as_bytes();
        let eth_address_result = EcPrivateKey::eth_wallet_address_from_public_key(public_key_bytes);
        if eth_address_result.is_err() {
            return Err(eth_address_result.err().unwrap().to_string());
        }
        let eth_address = eth_address_result.unwrap();
        println!("Recovered address: {}", &eth_address);

        // Compare both in lower case
        if address.to_lowercase() != eth_address.to_lowercase() {
            return Err("Recovered address does not match the address from the proto".to_string());
        }
        // Check recovered key == public key
//        if recovered_key != self.public_key {
//            return Err("Recovered key does not match public key".to_string());
//        }
        // Check signature
        if recovered_key.verify(message, &ec_signature).is_err() {
            return Err("Signature verification failed".to_string());
        }
        return Ok(());
    }

    // Verify signature
    pub fn verify_signature(&self, message: &[u8], signature: &[u8]) -> Result<(), String> {
        // Parse signature from raw compressed bytes
        let signature_result = Signature::try_from(signature);
        // Check signature_result
        if signature_result.is_err() {
            return Err(signature_result.err().unwrap().to_string());
        }
        let signature = signature_result.unwrap();

        // Verifying key from self.public_key
        let verifying_key = VerifyingKey::from(&self.public_key);
        // Verify signature
        let verify_result = verifying_key.verify(message, &signature);
        // Check verify_result
        if verify_result.is_err() {
            return Err(verify_result.err().unwrap().to_string());
        }
        return Ok(verify_result.unwrap());
    }
}
