use k256::ecdsa::signature::DigestVerifier;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdsa::{signature::Verifier, RecoveryId, Signature, VerifyingKey},
    PublicKey,
};
use sha2::Sha256;
use sha3::{Digest, Keccak256};

use crate::proto;
use crate::signature;
use crate::traits::{
    BridgeSignableVersion, Buffable, ECDHKey, Sha256SignatureVerifier, SignatureVerifiable,
    SignedECDHKey,
};
use protobuf::{Message, MessageField};

#[derive(Debug, Clone)]
pub struct SignedPublicKey {
    pub public_key: PublicKey,
    pub signed_bytes: Vec<u8>,
    pub signature: signature::Signature,
    pub created_at: u64,
}

pub fn recover_wallet_public_key(
    message: &[u8],
    signature: &proto::signature::Signature,
) -> Result<PublicKey, String> {
    // Expect ecdsa_compact field with subfields: bytes, recovery_id
    if !signature.has_wallet_ecdsa_compact() {
        return Err("No wallet_ecdsa_compact field found".to_string());
    }
    let wallet_ecdsa_compact = signature.wallet_ecdsa_compact();
    let signature_bytes = wallet_ecdsa_compact.bytes.as_slice();
    let recovery_id_result = RecoveryId::try_from(wallet_ecdsa_compact.recovery as u8);
    if recovery_id_result.is_err() {
        return Err(recovery_id_result.err().unwrap().to_string());
    }
    let recovery_id = recovery_id_result.unwrap();
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

    // First extract the public key from the recovered key
    let public_key = PublicKey::from(&recovered_key);

    // Finally use the recovered key in a re-verification, may not strictly be required
    if VerifyingKey::from(&public_key)
        .verify_digest(Keccak256::new_with_prefix(&message), &ec_signature)
        .is_err()
    {
        return Err("Signature verification failed".to_string());
    }
    return Ok(public_key);
}

// Need to do two layers of proto deserialization, key_bytes is just the bytes of the PublicKey proto
pub fn signed_public_key_from_proto(
    proto: &proto::public_key::SignedPublicKey,
) -> Result<PublicKey, String> {
    let mut public_key_proto_bytes = proto.key_bytes.as_slice();
    let public_key_proto_result: Result<proto::public_key::PublicKey, protobuf::Error> =
        protobuf::Message::parse_from_bytes(&mut public_key_proto_bytes);
    if public_key_proto_result.is_err() {
        return Err(public_key_proto_result.err().unwrap().to_string());
    }
    let public_key_result = PublicKey::from_sec1_bytes(
        public_key_proto_result
            .unwrap()
            .secp256k1_uncompressed()
            .bytes
            .as_slice(),
    );

    if public_key_result.is_err() {
        return Err(format!(
            "Error parsing sec1 bytes: {}",
            public_key_result.err().unwrap().to_string()
        ));
    }
    return Ok(public_key_result.unwrap());
}

pub fn signed_public_key_from_proto_v2(
    proto: &proto::public_key::SignedPublicKey,
) -> Result<SignedPublicKey, String> {
    let mut public_key_proto_bytes = proto.key_bytes.as_slice();
    let public_key_proto_result: Result<proto::public_key::PublicKey, protobuf::Error> =
        protobuf::Message::parse_from_bytes(&mut public_key_proto_bytes);
    if public_key_proto_result.is_err() {
        return Err(public_key_proto_result.err().unwrap().to_string());
    }
    let public_key_result = PublicKey::from_sec1_bytes(
        public_key_proto_result
            .unwrap()
            .secp256k1_uncompressed()
            .bytes
            .as_slice(),
    );

    // Extract the signature
    let signature_bytes = proto
        .signature
        .write_to_bytes()
        .map_err(|e| e.to_string())?;
    let signature = signature::Signature::from_proto_bytes(&signature_bytes)?;

    return Ok(SignedPublicKey {
        public_key: public_key_result.unwrap(),
        signed_bytes: proto.key_bytes.clone(),
        signature,
        created_at: 0,
    });
}

pub fn public_key_from_proto(proto: &proto::public_key::PublicKey) -> Result<PublicKey, String> {
    let public_key_bytes = proto.secp256k1_uncompressed().bytes.as_slice();
    let public_key_result = PublicKey::from_sec1_bytes(public_key_bytes);
    if public_key_result.is_err() {
        return Err(public_key_result.err().unwrap().to_string());
    }
    return Ok(public_key_result.unwrap());
}

pub fn to_unsigned_public_key_proto(
    public_key: &PublicKey,
    created_at: u64,
) -> proto::public_key::UnsignedPublicKey {
    // First, create the UnsignedPublicKey and set the secp256k1_uncompressed field
    let mut unsigned_public_key = proto::public_key::UnsignedPublicKey::new();

    // Get the uncompressed bytes of the public key
    let binding = public_key.to_encoded_point(false);
    let public_key_bytes = binding.as_bytes();
    let mut secp256k1_uncompressed =
        proto::public_key::unsigned_public_key::Secp256k1Uncompressed::new();
    secp256k1_uncompressed.bytes = public_key_bytes.to_vec();
    unsigned_public_key.set_secp256k1_uncompressed(secp256k1_uncompressed);
    unsigned_public_key.created_ns = created_at;
    return unsigned_public_key;
}

pub fn to_signed_public_key_proto(
    public_key: &PublicKey,
    created_at: u64,
) -> proto::public_key::SignedPublicKey {
    // First, get the UnsignedPublicKey proto
    let unsigned_public_key = to_unsigned_public_key_proto(public_key, created_at);

    let mut signed_public_key = proto::public_key::SignedPublicKey::new();
    signed_public_key.key_bytes = unsigned_public_key.write_to_bytes().unwrap();
    // TODO: STOPSHIP: Need to set the Signature
    return signed_public_key;
}

impl PartialEq for SignedPublicKey {
    fn eq(&self, other: &SignedPublicKey) -> bool {
        self.public_key == other.public_key
    }
}

impl ECDHKey for SignedPublicKey {
    fn get_public_key(&self) -> PublicKey {
        self.to_unsigned()
    }
}

impl BridgeSignableVersion<PublicKey, SignedPublicKey> for PublicKey {
    fn to_signed(&self) -> SignedPublicKey {
        SignedPublicKey {
            public_key: self.clone(),
            signature: signature::Signature::default(),
            signed_bytes: vec![],
            created_at: 0,
        }
    }

    fn to_unsigned(&self) -> PublicKey {
        self.clone()
    }
}

impl BridgeSignableVersion<PublicKey, SignedPublicKey> for SignedPublicKey {
    fn to_signed(&self) -> SignedPublicKey {
        self.clone()
    }

    fn to_unsigned(&self) -> PublicKey {
        self.public_key.clone()
    }
}

impl SignatureVerifiable for SignedPublicKey {
    fn get_signature(&self) -> Option<signature::Signature> {
        Some(self.signature.clone())
    }
}

// TODO: STOPSHIP: eliminate this trait when migration is complete
impl SignatureVerifiable for PublicKey {
    fn get_signature(&self) -> Option<signature::Signature> {
        None
    }
}

impl SignedECDHKey for SignedPublicKey {}
impl SignedECDHKey for PublicKey {}

impl Buffable for PublicKey {
    fn to_proto_bytes(&self) -> Result<Vec<u8>, String> {
        let unsigned_public_key_proto = to_unsigned_public_key_proto(self, 0);
        return unsigned_public_key_proto
            .write_to_bytes()
            .map_err(|e| e.to_string());
    }

    fn from_proto_bytes(buff: &[u8]) -> Result<Self, String> {
        let proto: proto::public_key::PublicKey =
            if let Ok(proto) = protobuf::Message::parse_from_bytes(buff) {
                proto
            } else {
                return Err("Error parsing PublicKey from bytes".to_string());
            };
        let public_key_bytes = proto.secp256k1_uncompressed().bytes.as_slice();
        let public_key_result = PublicKey::from_sec1_bytes(public_key_bytes);
        if public_key_result.is_err() {
            return Err(public_key_result.err().unwrap().to_string());
        }
        return Ok(public_key_result.unwrap());
    }
}

impl Sha256SignatureVerifier for PublicKey {
    // TODO: move away from [u8] to using real Signature types
    fn verify_sha256_signature(&self, message: &[u8], signature: &[u8]) -> Result<bool, String> {
        // Parse signature from raw compressed bytes
        let signature_result = Signature::try_from(signature);
        // Check signature_result
        if signature_result.is_err() {
            return Err(signature_result.err().unwrap().to_string());
        }
        let signature = signature_result.unwrap();

        // Verifying key from self.public_key
        let verifying_key = VerifyingKey::from(self);
        // Verify signature
        let verify_result = verifying_key.verify(message, &signature);
        // Check verify_result
        if verify_result.is_err() {
            return Err(verify_result.err().unwrap().to_string());
        }
        return Ok(true);
    }
}
