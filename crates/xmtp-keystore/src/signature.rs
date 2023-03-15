use crate::proto;
use crate::traits::Buffable;
use protobuf::Message;

enum SignatureType {
    ecdsa_secp256k1_sha256_compact = 1,
    wallet_personal_sign_compact = 2,
}

pub struct Signature {
    signature_type: SignatureType,
    signature_bytes: Box<[u8]>,
    recovery_id: Option<u8>,
}
