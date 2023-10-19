use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

pub fn identity_to_wallet_address(identity: &[u8], pub_key: &[u8]) -> String {
    let proto_value = Eip191AssociationProto::decode(identity).expect("failed to deserialize");
    let association = Eip191Association::from_proto_with_expected_address(
        pub_key,
        proto_value.clone(),
        proto_value.wallet_address,
    )
    .expect("failed to validate identity signature");

    association.address()
}
