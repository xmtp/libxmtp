use crate::xmtp::mls::api::v1::KeyPackageUpload;
use crate::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, UnsignedOriginatorEnvelope,
};
use openmls::key_packages::KeyPackageIn;
use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::{MlsMessageIn, ProtocolMessage, ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use prost::Message;
use crate::api_client;
use crate::types::TopicKind;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub fn build_key_package_topic(installation_id: &[u8]) -> Vec<u8> {

    let mut topic = Vec::with_capacity(1 + installation_id.len());
    topic.push(TopicKind::KeyPackagesV1 as u8);
    topic.extend_from_slice(installation_id);
    topic
}

pub fn build_identity_update_topic(inbox_id: &[u8]) -> Vec<u8> {
    let mut topic = Vec::with_capacity(1 + inbox_id.len());
    topic.push(TopicKind::IdentityUpdatesV1 as u8);
    topic.extend_from_slice(inbox_id);
    topic
}

pub fn build_group_message_topic(group_id: &[u8]) -> Vec<u8> {
    let mut topic = Vec::with_capacity(1 + group_id.len());
    topic.push(TopicKind::GroupMessagesV1 as u8);
    topic.extend_from_slice(group_id);
    topic
}

pub fn build_welcome_message_topic(installation_id: &[u8]) -> Vec<u8> {
    let mut topic = Vec::with_capacity(1 + installation_id.len());
    topic.push(TopicKind::WelcomeMessagesV1 as u8);
    topic.extend_from_slice(installation_id);
    topic
}

pub fn build_identity_topic_from_hex_encoded(hex_encoded_inbox_id: &String) -> Result<Vec<u8>, api_client::Error> {
    let decoded_inbox_id = hex::decode(hex_encoded_inbox_id)?;
    Ok(build_identity_update_topic(&decoded_inbox_id))
}

pub fn extract_unsigned_originator_envelope(
    req: &OriginatorEnvelope,
) -> UnsignedOriginatorEnvelope {
    let mut unsigned_bytes = req.unsigned_originator_envelope.as_slice();
    UnsignedOriginatorEnvelope::decode(&mut unsigned_bytes)
        .expect("Failed to decode unsigned originator envelope")
}

pub fn extract_client_envelope(req: &OriginatorEnvelope) -> ClientEnvelope {
    let unsigned_originator = extract_unsigned_originator_envelope(req);

    let payer_envelope = unsigned_originator.payer_envelope.unwrap();
    let mut payer_bytes = payer_envelope.unsigned_client_envelope.as_slice();
    ClientEnvelope::decode(&mut payer_bytes).expect("Failed to decode client envelope")
}

pub fn extract_group_id_from_topic(topic: Vec<u8>) -> Vec<u8> {
    let topic_str = String::from_utf8(topic).expect("Failed to convert topic to string");
    let group_id = topic_str
        .split("/")
        .nth(1)
        .expect("Failed to extract group id from topic");
    group_id.as_bytes().to_vec()
}

pub fn get_group_message_topic(message: Vec<u8>) -> Vec<u8> {
    let msg_result = MlsMessageIn::tls_deserialize(&mut message.as_slice())
        .expect("Failed to deserialize message");

    let protocol_message: ProtocolMessage = msg_result
        .try_into_protocol_message()
        .expect("Failed to convert to protocol message");

    build_group_message_topic(protocol_message.group_id().as_slice())
}

pub fn get_key_package_topic(key_package: &KeyPackageUpload) -> Vec<u8> {
    let kp_in: KeyPackageIn =
        KeyPackageIn::tls_deserialize_exact(key_package.key_package_tls_serialized.as_slice())
            .expect("key package serialization");
    let rust_crypto = RustCrypto::default();
    let kp = kp_in
        .validate(&rust_crypto, MLS_PROTOCOL_VERSION)
        .expect("key package validation");
    let installation_key = kp.leaf_node().signature_key().as_slice();
    build_key_package_topic(installation_key)
}
