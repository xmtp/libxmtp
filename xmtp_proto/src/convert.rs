use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion, GroupMessageInput, KeyPackageUpload,
    UploadKeyPackageRequest,
};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{
    AuthenticatedData, ClientEnvelope, OriginatorEnvelope, UnsignedOriginatorEnvelope,
};
use crate::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;
use openmls::key_packages::KeyPackageIn;
use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::{MlsMessageIn, ProtocolMessage, ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use prost::Message;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1,
    IdentityUpdatesV1,
    KeyPackagesV1,
}

mod inbox_id {

    use crate::xmtp::identity::MlsCredential;
    use openmls::{
        credentials::{errors::BasicCredentialError, BasicCredential},
        prelude::Credential as OpenMlsCredential,
    };
    use prost::Message;

    impl TryFrom<MlsCredential> for OpenMlsCredential {
        type Error = BasicCredentialError;

        fn try_from(proto: MlsCredential) -> Result<OpenMlsCredential, Self::Error> {
            let bytes = proto.encode_to_vec();
            Ok(BasicCredential::new(bytes).into())
        }
    }
}

impl From<UploadKeyPackageRequest> for PublishClientEnvelopesRequest {
    fn from(req: UploadKeyPackageRequest) -> Self {
        PublishClientEnvelopesRequest {
            envelopes: vec![ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(get_key_package_topic(
                    req.key_package.as_ref().unwrap(),
                ))),
                payload: Some(Payload::UploadKeyPackage(req)),
            }],
        }
    }
}

impl From<PublishIdentityUpdateRequest> for PublishClientEnvelopesRequest {
    fn from(req: PublishIdentityUpdateRequest) -> Self {
        let identity_update = req.identity_update.unwrap();
        PublishClientEnvelopesRequest {
            envelopes: vec![ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(build_identity_update_topic(
                    identity_update.inbox_id.clone(),
                ))),
                payload: Some(Payload::IdentityUpdate(identity_update)),
            }],
        }
    }
}

impl From<GroupMessageInput> for PublishClientEnvelopesRequest {
    fn from(req: GroupMessageInput) -> Self {
        let GroupMessageInputVersion::V1(version) = req.version.as_ref().unwrap();

        PublishClientEnvelopesRequest {
            envelopes: vec![ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(get_group_message_topic(
                    version.data.clone(),
                ))),
                payload: Some(Payload::GroupMessage(req)),
            }],
        }
    }
}

impl AuthenticatedData {
    pub fn with_topic(topic: Vec<u8>) -> AuthenticatedData {
        AuthenticatedData {
            target_originator: 100,
            target_topic: topic,
            last_seen: None,
        }
    }
}

fn get_key_package_topic(key_package: &KeyPackageUpload) -> Vec<u8> {
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

pub fn build_key_package_topic(installation_id: &[u8]) -> Vec<u8> {
    [
        vec![TopicKind::KeyPackagesV1 as u8],
        Vec::from(installation_id),
    ]
    .concat()
}

pub fn build_identity_update_topic(inbox_id: String) -> Vec<u8> {
    [
        vec![TopicKind::IdentityUpdatesV1 as u8],
        hex::decode(inbox_id).unwrap(),
    ]
    .concat()
}

pub fn build_group_message_topic(group_id: &[u8]) -> Vec<u8> {
    [vec![TopicKind::GroupMessagesV1 as u8], Vec::from(group_id)].concat()
}

pub fn build_welcome_message_topic(installation_id: &[u8]) -> Vec<u8> {
    [
        vec![TopicKind::WelcomeMessagesV1 as u8],
        Vec::from(installation_id),
    ]
    .concat()
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

fn get_group_message_topic(message: Vec<u8>) -> Vec<u8> {
    let msg_result = MlsMessageIn::tls_deserialize(&mut message.as_slice())
        .expect("Failed to deserialize message");

    let protocol_message: ProtocolMessage = msg_result
        .try_into_protocol_message()
        .expect("Failed to convert to protocol message");

    build_group_message_topic(protocol_message.group_id().as_slice())
}
