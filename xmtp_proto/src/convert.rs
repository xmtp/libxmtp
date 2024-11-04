use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::{KeyPackageUpload, UploadKeyPackageRequest};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{AuthenticatedData, ClientEnvelope};
use openmls::key_packages::KeyPackageIn;
use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::ProtocolVersion;
use openmls_rust_crypto::RustCrypto;

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
        format!("kp/{}", hex::encode(installation_id)).into_bytes(),
    ]
    .concat()
}

pub fn build_identity_update_topic(inbox_id: String) -> Vec<u8> {
    [
        vec![TopicKind::IdentityUpdatesV1 as u8],
        format!("i/{}", inbox_id).into_bytes(),
    ]
    .concat()
}
