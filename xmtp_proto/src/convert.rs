use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion,
    welcome_message_input::Version as WelcomeMessageVersion, GroupMessageInput,
    UploadKeyPackageRequest, WelcomeMessageInput,
};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{
    AuthenticatedData, ClientEnvelope,
};
use crate::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;

use crate::{Error};
use crate::ErrorKind::InternalError;
use crate::InternalError::{MissingPayloadError};
use crate::v4_utils::{build_identity_topic_from_hex_encoded, build_welcome_message_topic, get_group_message_topic, get_key_package_topic};

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

impl TryFrom<UploadKeyPackageRequest> for PublishClientEnvelopesRequest {
    type Error = Error;

    fn try_from(req: UploadKeyPackageRequest) -> Result<Self, Error> {
        if let Some(key_package) = req.key_package.as_ref() {
            Ok(PublishClientEnvelopesRequest {
                envelopes: vec![ClientEnvelope {
                    aad: Some(AuthenticatedData::with_topic(get_key_package_topic(
                        key_package,
                    ))),
                    payload: Some(Payload::UploadKeyPackage(req)),
                }],
            })
        } else {
            Err(Error::new(InternalError(MissingPayloadError)))
        }
    }
}

impl TryFrom<PublishIdentityUpdateRequest> for PublishClientEnvelopesRequest {
    type Error = Error;

    fn try_from(req: PublishIdentityUpdateRequest) -> Result<Self, Error> {
        if let Some(identity_update) = req.identity_update {
            Ok(PublishClientEnvelopesRequest {
                envelopes: vec![ClientEnvelope {
                    aad: Some(AuthenticatedData::with_topic(build_identity_topic_from_hex_encoded(
                        &identity_update.inbox_id
                    )?)),
                    payload: Some(Payload::IdentityUpdate(identity_update)),
                }],
            })
        } else {
            Err(Error::new(InternalError(MissingPayloadError)))
        }
    }
}

impl TryFrom<GroupMessageInput> for PublishClientEnvelopesRequest {
    type Error = crate::Error;

    fn try_from(req: GroupMessageInput) -> Result<Self, Error> {
        if let Some(GroupMessageInputVersion::V1(ref version)) = req.version {
            Ok(PublishClientEnvelopesRequest {
                envelopes: vec![ClientEnvelope {
                    aad: Some(AuthenticatedData::with_topic(get_group_message_topic(
                        version.data.clone(),
                    ))),
                    payload: Some(Payload::GroupMessage(req)),
                }],
            })
        } else {
            Err(Error::new(InternalError(MissingPayloadError)))
        }
    }
}

impl TryFrom<WelcomeMessageInput> for PublishClientEnvelopesRequest {
    type Error = crate::Error;

    fn try_from(req: WelcomeMessageInput) -> Result<Self, Self::Error> {
        if let Some(WelcomeMessageVersion::V1(ref version)) = req.version {
            Ok(PublishClientEnvelopesRequest {
                envelopes: vec![ClientEnvelope {
                    aad: Some(AuthenticatedData::with_topic(build_welcome_message_topic(
                        version.installation_key.as_slice(),
                    ))),
                    payload: Some(Payload::WelcomeMessage(req)),
                }],
            })
        } else {
            Err(Error::new(InternalError(MissingPayloadError)))
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