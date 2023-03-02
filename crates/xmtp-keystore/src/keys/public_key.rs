use k256::PublicKey;
use k256::elliptic_curve::sec1::ToEncodedPoint;

use super::super::proto;
use protobuf;

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
        return Err(public_key_result.err().unwrap().to_string());
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

// // UnsignedPublicKey represents a generalized public key,
// // defined as a union to support cryptographic algorithm agility.
// message UnsignedPublicKey {
//     uint64 created_ns = 1;
//     oneof union {
//         Secp256k1Uncompressed secp256k1_uncompressed = 3;
//     }
// 
//     // Supported key types
// 
//     // EC: SECP256k1
//     message Secp256k1Uncompressed {
//         // uncompressed point with prefix (0x04) [ P || X || Y ], 65 bytes
//         bytes bytes = 1; 
//     }
// }
// message SignedPublicKey {
//     bytes key_bytes = 1;  // embeds an UnsignedPublicKey
//     Signature signature = 2; // signs key_bytes, legacy association proof
//     AssociationProof proof = 3; // proves association with a user identity
// }
pub fn public_key_to_proto(public_key: &PublicKey) -> proto::public_key::PublicKey {
    // First, create the UnsignedPublicKey and set the secp256k1_uncompressed field
    let mut unsigned_public_key = proto::public_key::UnsignedPublicKey::new();

    // Get the uncompressed bytes of the public key
    let public_key_bytes = public_key.to_encoded_point(false).as_bytes();
    let mut secp256k1_uncompressed = proto::public_key::unsigned_public_key::Secp256k1Uncompressed(
        public_key_bytes.to_vec(),
    );
    unsigned_public_key.secp256k1_uncompressed = secp256k1_uncompressed;
    // TODO: STOPSHIP: the created timestamp needs to be carried with the signature
    unsigned_public_key.created_ns = 0;

    let mut signed_public_key = proto::public_key::SignedPublicKey::new();
    signed_public_key.set_key_bytes(unsigned_public_key.write_to_bytes().unwrap());
    return signed_public_key;
}
