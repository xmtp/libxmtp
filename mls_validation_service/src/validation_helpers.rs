use crate::association::Eip191Association;
use xmtp_cryptography::hash::keccak256;

use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

pub fn base64_encode(bytes: &[u8]) -> String {
    general_purpose::STANDARD_NO_PAD.encode(bytes)
}

pub fn pub_key_to_installation_id(key: &[u8]) -> String {
    base64_encode(keccak256(key.to_string().as_str()).as_slice())
}

pub fn identity_to_wallet_address(identity: &[u8], pub_key: &[u8]) -> Result<String, String> {
    let proto_value = Eip191AssociationProto::decode(identity).map_err(|e| format!("{:?}", e))?;
    let association = Eip191Association::from_proto_with_expected_address(
        pub_key,
        proto_value.clone(),
        proto_value.wallet_address,
    )
    .map_err(|e| format!("{:?}", e))?;

    Ok(association.address())
}
