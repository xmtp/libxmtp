use prost::Message;
use xmtp_mls::association::Credential;
use xmtp_proto::xmtp::mls::message_contents::MlsCredential as CredentialProto;

pub fn identity_to_account_address(identity: &[u8], pub_key: &[u8]) -> Result<String, String> {
    let proto = CredentialProto::decode(identity).map_err(|e| format!("{:?}", e))?;
    let credential = Credential::from_proto_validated(
        proto,
        None, // expected_account_address,
        Some(pub_key),
    )
    .map_err(|e| format!("{:?}", e))?;

    Ok(credential.address())
}
