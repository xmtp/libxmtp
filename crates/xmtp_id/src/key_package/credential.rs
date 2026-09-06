use openmls::credentials::BasicCredential;
use openmls::prelude::Credential;
use prost::Message;
use xmtp_proto::xmtp::identity::MlsCredential;

pub fn create_credential(inbox_id: impl AsRef<str>) -> Credential {
    BasicCredential::new(
        MlsCredential {
            inbox_id: inbox_id.as_ref().to_owned(),
        }
        .encode_to_vec(),
    )
    .into()
}

pub fn parse_credential(bytes: &[u8]) -> Result<crate::InboxId, prost::DecodeError> {
    Ok(MlsCredential::decode(bytes)?.inbox_id)
}
