//! Builder to build a mocked `OriginatorEnvelope`
use prost::Message;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::{
    GroupMessageInput, UploadKeyPackageRequest, WelcomeMessageInput, group_message_input,
    welcome_message_input,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    AuthenticatedData, ClientEnvelope, OriginatorEnvelope, PayerEnvelope,
    UnsignedOriginatorEnvelope, client_envelope::Payload,
};

pub const MOCK_MLS_MESSAGE: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
pub const MOCK_KEY_PACKAGE: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Builder for creating test OriginatorEnvelopes with sensible defaults
#[derive(Clone, Default)]
pub struct TestEnvelopeBuilder {
    originator_node_id: u32,
    originator_sequence_id: u64,
    originator_ns: i64,
    target_originator: u32,
    message_retention_days: u32,
    base_fee_picodollars: u64,
    congestion_fee_picodollars: u64,
    expiry_unixtime: u64,
    target_topic: Vec<u8>,
    payload: Option<Payload>,
}

impl TestEnvelopeBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_originator_node_id(mut self, node_id: u32) -> Self {
        self.originator_node_id = node_id;
        self
    }

    pub fn with_originator_sequence_id(mut self, sequence_id: u64) -> Self {
        self.originator_sequence_id = sequence_id;
        self
    }

    pub fn with_originator_ns(mut self, ns: i64) -> Self {
        self.originator_ns = ns;
        self
    }

    pub fn with_group_message(self) -> Self {
        self.with_group_message_custom(MOCK_MLS_MESSAGE.to_vec(), vec![])
    }

    pub fn with_group_message_custom(mut self, data: Vec<u8>, sender_hmac: Vec<u8>) -> Self {
        self.payload = Some(Payload::GroupMessage(GroupMessageInput {
            version: Some(group_message_input::Version::V1(group_message_input::V1 {
                data,
                sender_hmac,
                should_push: true,
            })),
        }));
        self
    }

    pub fn with_welcome_message(self) -> Self {
        self.with_welcome_message_custom(vec![5, 6, 7, 8])
    }

    pub fn with_welcome_message_custom(mut self, installation_key: Vec<u8>) -> Self {
        self.payload = Some(Payload::WelcomeMessage(WelcomeMessageInput {
            version: Some(welcome_message_input::Version::V1(
                welcome_message_input::V1 {
                    installation_key,
                    data: vec![],
                    hpke_public_key: vec![],
                    wrapper_algorithm: 1,
                    welcome_metadata: vec![],
                },
            )),
        }));
        self
    }

    pub fn with_welcome_message_detailed(
        self,
        installation_key: Vec<u8>,
        data: Vec<u8>,
        hpke_public_key: Vec<u8>,
    ) -> Self {
        self.with_welcome_message_full(installation_key, data, hpke_public_key, 1, vec![])
    }

    pub fn with_welcome_message_full(
        mut self,
        installation_key: Vec<u8>,
        data: Vec<u8>,
        hpke_public_key: Vec<u8>,
        wrapper_algorithm: i32,
        welcome_metadata: Vec<u8>,
    ) -> Self {
        self.payload = Some(Payload::WelcomeMessage(WelcomeMessageInput {
            version: Some(welcome_message_input::Version::V1(
                welcome_message_input::V1 {
                    installation_key,
                    data,
                    hpke_public_key,
                    wrapper_algorithm,
                    welcome_metadata,
                },
            )),
        }));
        self
    }

    pub fn with_key_package(self) -> Self {
        self.with_key_package_custom(MOCK_KEY_PACKAGE.to_vec())
    }

    pub fn with_key_package_custom(mut self, key_package_data: Vec<u8>) -> Self {
        self.payload = Some(Payload::UploadKeyPackage(UploadKeyPackageRequest {
            key_package: Some(xmtp_proto::mls_v1::KeyPackageUpload {
                key_package_tls_serialized: key_package_data,
            }),
            is_inbox_id_credential: false,
        }));
        self
    }

    pub fn with_invalid_key_package(mut self) -> Self {
        self.payload = Some(Payload::UploadKeyPackage(UploadKeyPackageRequest {
            key_package: None,
            is_inbox_id_credential: false,
        }));
        self
    }

    pub fn with_identity_update(self) -> Self {
        self.with_identity_update_custom("abcd1234".to_string())
    }

    pub fn with_identity_update_custom(mut self, inbox_id: String) -> Self {
        self.payload = Some(Payload::IdentityUpdate(IdentityUpdate {
            actions: vec![],
            client_timestamp_ns: 0,
            inbox_id,
        }));
        self
    }

    pub fn with_invalid_identity_update(self) -> Self {
        self.with_identity_update_custom("invalid_hex!@#".to_string())
    }

    pub fn with_empty_payload(mut self) -> Self {
        self.payload = None;
        self
    }

    pub fn build(self) -> OriginatorEnvelope {
        OriginatorEnvelope {
            unsigned_originator_envelope: UnsignedOriginatorEnvelope {
                originator_node_id: self.originator_node_id,
                originator_sequence_id: self.originator_sequence_id,
                originator_ns: self.originator_ns,
                payer_envelope_bytes: PayerEnvelope {
                    unsigned_client_envelope: ClientEnvelope {
                        aad: Some(AuthenticatedData {
                            target_topic: self.target_topic,
                            depends_on: None,
                        }),
                        payload: self.payload,
                    }
                    .encode_to_vec(),
                    payer_signature: None,
                    target_originator: self.target_originator,
                    message_retention_days: self.message_retention_days,
                }
                .encode_to_vec(),
                base_fee_picodollars: self.base_fee_picodollars,
                congestion_fee_picodollars: self.congestion_fee_picodollars,
                expiry_unixtime: self.expiry_unixtime,
            }
            .encode_to_vec(),
            proof: None,
        }
    }
}
