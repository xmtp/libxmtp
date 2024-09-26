use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::{KeyPackageIn, MlsMessageIn, ProtocolMessage, ProtocolVersion};

use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion,
    welcome_message_input::Version as WelcomeMessageVersion,
};
use crate::xmtp::mls::api::v1::{
    GroupMessageInput, KeyPackageUpload, UploadKeyPackageRequest, WelcomeMessageInput,
};
use crate::xmtp::xmtpv4::client_envelope::Payload;
use crate::xmtp::xmtpv4::{AuthenticatedData, ClientEnvelope};
use openmls_rust_crypto::RustCrypto;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

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

impl From<WelcomeMessageInput> for ClientEnvelope {
    fn from(req: WelcomeMessageInput) -> Self {
        ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(get_welcome_message_topic(
                &req,
            ))),
            payload: Some(Payload::WelcomeMessage(req)),
        }
    }
}

impl From<GroupMessageInput> for ClientEnvelope {
    fn from(req: GroupMessageInput) -> Self {
        let version = match req.version.as_ref().unwrap() {
            GroupMessageInputVersion::V1(v1) => v1,
        };
        let topic = get_group_message_topic(version.data.clone());
        ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(topic)),
            payload: Some(Payload::GroupMessage(req)),
        }
    }
}

impl From<UploadKeyPackageRequest> for ClientEnvelope {
    fn from(req: UploadKeyPackageRequest) -> Self {
        ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(get_key_package_topic(
                &req.key_package.as_ref().unwrap(),
            ))),
            payload: Some(Payload::UploadKeyPackage(req)),
        }
    }
}

impl From<PublishIdentityUpdateRequest> for ClientEnvelope {
    fn from(req: PublishIdentityUpdateRequest) -> Self {
        let identity_update = req.identity_update.unwrap();
        ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(build_identity_update_topic(
                identity_update.inbox_id.clone(),
            ))),
            payload: Some(Payload::IdentityUpdate(identity_update)),
        }
    }
}

impl AuthenticatedData {
    pub fn dummy() -> AuthenticatedData {
        AuthenticatedData {
            target_originator: 1,
            target_topic: vec![0x5],
            last_seen: None,
        }
    }

    pub fn with_topic(topic: Vec<u8>) -> AuthenticatedData {
        AuthenticatedData {
            target_originator: 1,
            target_topic: topic,
            last_seen: None,
        }
    }
}

fn get_group_message_topic(message: Vec<u8>) -> Vec<u8> {
    let msg_result = MlsMessageIn::tls_deserialize(&mut message.as_slice())
        .expect("Failed to deserialize message");

    let protocol_message: ProtocolMessage = msg_result
        .try_into_protocol_message()
        .expect("Failed to convert to protocol message");

    build_group_message_topic(protocol_message.group_id().as_slice())
}

fn get_welcome_message_topic(req: &WelcomeMessageInput) -> Vec<u8> {
    let WelcomeMessageVersion::V1(v1) = req.version.as_ref().unwrap();
    build_welcome_message_topic(v1.installation_key.as_slice())
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

pub fn build_identity_update_topic(inbox_id: String) -> Vec<u8> {
    format!("i/{}", inbox_id).into_bytes()
}

pub fn build_key_package_topic(installation_id: &[u8]) -> Vec<u8> {
    format!("kp/{}", hex::encode(installation_id)).into_bytes()
}

fn build_welcome_message_topic(installation_id: &[u8]) -> Vec<u8> {
    format!("w/{}", hex::encode(installation_id)).into_bytes()
}

pub fn build_group_message_topic(group_id: &[u8]) -> Vec<u8> {
    format!("g/{}", hex::encode(group_id)).into_bytes()
}
