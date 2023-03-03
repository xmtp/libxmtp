use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;

use super::super::proto;
use protobuf::{Message, MessageField};

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
