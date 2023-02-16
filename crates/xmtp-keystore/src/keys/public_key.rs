use k256::PublicKey;

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
    let mut public_key_bytes = proto.secp256k1_uncompressed().bytes.as_slice();
    let public_key_result = PublicKey::from_sec1_bytes(public_key_bytes);
    if public_key_result.is_err() {
        return Err(public_key_result.err().unwrap().to_string());
    }
    return Ok(public_key_result.unwrap());
}
