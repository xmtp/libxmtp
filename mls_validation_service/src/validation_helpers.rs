use prost::Message;
use xmtp_mls::association::{AssociationContext, Eip191Association};
use xmtp_proto::xmtp::mls::message_contents::Eip191Association as Eip191AssociationProto;

pub fn identity_to_account_address(identity: &[u8], pub_key: &[u8]) -> Result<String, String> {
    let proto_value = Eip191AssociationProto::decode(identity).map_err(|e| format!("{:?}", e))?;
    let association = Eip191Association::from_proto_with_expected_address(
        AssociationContext::GrantMessagingAccess,
        pub_key,
        proto_value.clone(),
        proto_value.account_address,
    )
    .map_err(|e| format!("{:?}", e))?;

    Ok(association.address())
}
