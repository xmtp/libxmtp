use crate::mls_v1::{self, GroupMessage, WelcomeMessage};
use crate::v4_utils::{
    build_identity_topic_from_hex_encoded, build_welcome_message_topic, get_group_message_topic,
    get_key_package_topic, Extract,
};
use crate::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;
use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion,
    welcome_message_input::Version as WelcomeMessageVersion, GroupMessageInput,
    UploadKeyPackageRequest, WelcomeMessageInput,
};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{AuthenticatedData, ClientEnvelope, OriginatorEnvelope};
use crate::ConversionError;
use openmls::prelude::tls_codec::Deserialize;
use openmls::{framing::MlsMessageIn, prelude::ProtocolMessage};

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

impl TryFrom<UploadKeyPackageRequest> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: UploadKeyPackageRequest) -> Result<Self, Self::Error> {
        if let Some(key_package) = req.key_package.as_ref() {
            let topic =
                get_key_package_topic(key_package).map_err(|_| ConversionError::Missing {
                    item: "topic",
                    r#type: std::any::type_name::<UploadKeyPackageRequest>(),
                })?;

            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(topic)),
                payload: Some(Payload::UploadKeyPackage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "key_package",
                r#type: std::any::type_name::<UploadKeyPackageRequest>(),
            })
        }
    }
}

impl TryFrom<OriginatorEnvelope> for KeyPackage {
    type Error = ConversionError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        let client_envelope = originator.client_envelope()?;

        if let Some(Payload::UploadKeyPackage(upload_key_package)) = client_envelope.payload {
            let key_package = upload_key_package
                .key_package
                .ok_or(ConversionError::Missing {
                    item: "key_package",
                    r#type: std::any::type_name::<OriginatorEnvelope>(),
                })?;

            Ok(KeyPackage {
                key_package_tls_serialized: key_package.key_package_tls_serialized,
            })
        } else {
            Err(ConversionError::Missing {
                item: "key_package",
                r#type: std::any::type_name::<OriginatorEnvelope>(),
            })
        }
    }
}

impl TryFrom<OriginatorEnvelope> for IdentityUpdateLog {
    type Error = ConversionError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        let unsigned_originator = originator.unsigned_originator_envelope()?;
        let client_envelope = originator.client_envelope()?;
        let payload = client_envelope.payload.ok_or(ConversionError::Missing {
            item: "identity_update",
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })?;

        let identity_update = match payload {
            Payload::IdentityUpdate(update) => update,
            _ => {
                return Err(ConversionError::Missing {
                    item: "identity_update",
                    r#type: std::any::type_name::<OriginatorEnvelope>(),
                });
            }
        };

        Ok(IdentityUpdateLog {
            sequence_id: unsigned_originator.originator_sequence_id,
            server_timestamp_ns: unsigned_originator.originator_ns as u64,
            update: Some(identity_update),
        })
    }
}
impl TryFrom<PublishIdentityUpdateRequest> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: PublishIdentityUpdateRequest) -> Result<Self, Self::Error> {
        if let Some(identity_update) = req.identity_update {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(
                    build_identity_topic_from_hex_encoded(&identity_update.inbox_id).map_err(
                        |_| ConversionError::Missing {
                            item: "identity_update",
                            r#type: std::any::type_name::<PublishIdentityUpdateRequest>(),
                        },
                    )?,
                )),
                payload: Some(Payload::IdentityUpdate(identity_update)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "identity_update",
                r#type: std::any::type_name::<PublishIdentityUpdateRequest>(),
            })
        }
    }
}

impl TryFrom<GroupMessageInput> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: GroupMessageInput) -> Result<Self, Self::Error> {
        if let Some(GroupMessageInputVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(
                    get_group_message_topic(version.data.clone()).map_err(|_| {
                        ConversionError::Missing {
                            item: "group_message",
                            r#type: std::any::type_name::<GroupMessageInput>(),
                        }
                    })?,
                )),
                payload: Some(Payload::GroupMessage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "group_message",
                r#type: std::any::type_name::<GroupMessageInput>(),
            })
        }
    }
}

impl TryFrom<WelcomeMessageInput> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: WelcomeMessageInput) -> Result<Self, Self::Error> {
        if let Some(WelcomeMessageVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(build_welcome_message_topic(
                    version.installation_key.as_slice(),
                ))),
                payload: Some(Payload::WelcomeMessage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "welcome_message",
                r#type: std::any::type_name::<WelcomeMessageInput>(),
            })
        }
    }
}

/// TODO: Move conversions to api_d14n
impl TryFrom<OriginatorEnvelope> for GroupMessage {
    type Error = ConversionError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        use mls_v1::group_message_input::Version;
        let unsigned_originator = originator.unsigned_originator_envelope()?;
        let client_envelope = originator.client_envelope()?;

        let payload = client_envelope.payload.ok_or(ConversionError::Missing {
            item: "payload",
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })?;

        let Payload::GroupMessage(msg_version) = payload else {
            return Err(ConversionError::InvalidValue {
                item: std::any::type_name::<OriginatorEnvelope>(),
                expected: "Payload::GroupMessage",
                got: payload.to_string(),
            });
        };

        // in case more versions are added
        #[allow(irrefutable_let_patterns)]
        let Version::V1(msg_in) = msg_version.version.ok_or(ConversionError::Missing {
            item: std::any::type_name::<Version>(),
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })?
        else {
            return Err(ConversionError::InvalidVersion);
        };

        let msg = MlsMessageIn::tls_deserialize(&mut msg_in.data.as_slice())?;
        let protocol_message: ProtocolMessage = msg.try_into_protocol_message()?;

        // TODO:insipx: we could easily extract more information here to make
        // processing messages easier
        // for instance, we have the epoch, group_id and data, and we can create a XmtpGruopMessage
        // struct to store this extra data rather than re-do deserialization
        // in 'process_message'
        // We can do that for v3 as well
        let msg_in = mls_v1::group_message::Version::V1(mls_v1::group_message::V1 {
            id: unsigned_originator.originator_sequence_id,
            created_ns: unsigned_originator.originator_ns as u64,
            group_id: protocol_message.group_id().to_vec(),
            data: msg_in.data,
            sender_hmac: msg_in.sender_hmac,
            should_push: msg_in.should_push,
        });
        Ok(mls_v1::GroupMessage {
            version: Some(msg_in),
        })
    }
}

// TODO:insipx: Can make conversion between originator and other types generic
impl TryFrom<OriginatorEnvelope> for WelcomeMessage {
    type Error = ConversionError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        use mls_v1::welcome_message_input::Version;
        let unsigned_originator = originator.unsigned_originator_envelope()?;
        let client_envelope = originator.client_envelope()?;

        let payload = client_envelope.payload.ok_or(ConversionError::Missing {
            item: "payload",
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })?;

        let Payload::WelcomeMessage(welcome_version) = payload else {
            return Err(ConversionError::InvalidValue {
                item: std::any::type_name::<OriginatorEnvelope>(),
                expected: "Payload::WelcomeMessage",
                got: payload.to_string(),
            });
        };

        // in case more versions are added
        #[allow(irrefutable_let_patterns)]
        let Version::V1(welcome_in) = welcome_version.version.ok_or(ConversionError::Missing {
            item: std::any::type_name::<Version>(),
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })?
        else {
            return Err(ConversionError::InvalidVersion);
        };

        let welcome_in = mls_v1::welcome_message::Version::V1(mls_v1::welcome_message::V1 {
            id: unsigned_originator.originator_sequence_id,
            created_ns: unsigned_originator.originator_ns as u64,
            installation_key: welcome_in.installation_key,
            data: welcome_in.data,
            hpke_public_key: welcome_in.hpke_public_key,
            wrapper_algorithm: welcome_in.wrapper_algorithm,

            // TODO: extend originator envelope to contain this info
            message_cursor: 0,
        });

        Ok(mls_v1::WelcomeMessage {
            version: Some(welcome_in),
        })
    }
}

impl AuthenticatedData {
    #[allow(deprecated)]
    pub fn with_topic(topic: Vec<u8>) -> AuthenticatedData {
        AuthenticatedData {
            target_originator: None,
            target_topic: topic,
            depends_on: None,
            is_commit: false,
        }
    }
}
