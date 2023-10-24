use xmtp_mls::association::Eip191Association;

use prost::Message;
use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

pub fn hex_encode(key: &[u8]) -> String {
    hex::encode(key)
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
