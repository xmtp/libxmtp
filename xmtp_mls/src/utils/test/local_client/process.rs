use super::*;
use crate::{types::InstallationId, verified_key_package_v2::VerifiedKeyPackageV2};
use openmls::prelude::{MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use std::sync::atomic::Ordering;
use tls_codec::Deserialize;
use xmtp_proto::{
    api_client::{MutableApiSubscription, XmtpApiClient, XmtpApiSubscription, XmtpMlsClient},
    identity::api::v1::prelude::*,
    mls::api::v1::prelude::*,
};

impl ProcessLocalRequest for UploadKeyPackageRequest {
    fn process(
        &self,
        _cursor: Option<&AtomicUsize>,
        _inbox_log: Option<LocalInboxLogs>,
    ) -> Result<State> {
        let rust_crypto = RustCrypto::default();
        let key_package = &self
            .key_package
            .as_ref()
            .unwrap()
            .key_package_tls_serialized;

        let kp = VerifiedKeyPackageV2::from_bytes(&rust_crypto, key_package.as_slice())?;

        let mut h = HashMap::new();
        h.insert(
            InstallationId::try_from(kp.installation_public_key.clone())
                .expect("Installation ID must be 32 bytes"),
            kp,
        );

        Ok(State {
            key_packages: h,
            ..Default::default()
        })
    }
}

impl ProcessLocalRequest for SendGroupMessagesRequest {
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        _inbox_log: Option<LocalInboxLogs>,
    ) -> Result<State> {
        use xmtp_proto::xmtp::mls::api::v1::group_message_input::Version;
        use xmtp_proto::xmtp::mls::api::v1::group_message_input::V1;
        let SendGroupMessagesRequest { messages } = self;
        let mut ret = HashMap::new();
        for message in messages.iter() {
            let GroupMessageInput {
                version: Some(Version::V1(V1 { data, sender_hmac })),
            } = message
            else {
                // skip if none
                continue;
            };

            let msg_result = MlsMessageIn::tls_deserialize(&mut data.as_slice())?;
            let protocol_message: ProtocolMessage = msg_result.try_into_protocol_message()?;
            ret.entry(protocol_message.group_id().clone().to_vec().into())
                .or_insert_with(Vec::new)
                .push(LocalGroupMessage {
                    id: cursor.map(|c| c.fetch_add(1, Ordering::SeqCst)).unwrap(),
                    msg: protocol_message,
                    data: data.clone(),
                    sender_hmac: sender_hmac.to_vec(),
                    created: xmtp_common::time::now_ns(),
                });
        }

        Ok(State {
            messages: ret,
            ..Default::default()
        })
    }
}

// id, created_at, installation_key, data, hpke_public_key, installation_key_data_hash
impl ProcessLocalRequest for SendWelcomeMessagesRequest {
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        _inbox_log: Option<LocalInboxLogs>,
    ) -> Result<State> {
        use xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version;
        use xmtp_proto::xmtp::mls::api::v1::welcome_message_input::V1;

        let SendWelcomeMessagesRequest { messages } = self;
        let mut ret = HashMap::new();

        for message in messages.iter() {
            let WelcomeMessageInput {
                version:
                    Some(Version::V1(V1 {
                        installation_key,
                        data,
                        hpke_public_key,
                    })),
            } = message
            else {
                continue;
            };
            ret.entry(installation_key.to_vec().try_into().unwrap())
                .or_insert_with(Vec::new)
                .push(LocalWelcomeMessage {
                    id: cursor.map(|c| c.fetch_add(1, Ordering::SeqCst)).unwrap(),
                    created_at: xmtp_common::time::now_ns(),
                    installation_key: installation_key.to_vec(),
                    hpke_public_key: hpke_public_key.to_vec(),
                    data: data.to_vec(),
                    installation_key_data_hash: xmtp_cryptography::hash::sha256_bytes(
                        vec![installation_key.clone(), data.clone()]
                            .concat()
                            .as_slice(),
                    ),
                })
        }

        Ok(State {
            welcomes: ret,
            ..Default::default()
        })
    }
}

impl ProcessLocalRequest for PublishIdentityUpdateRequest {
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        inbox_log: Option<LocalInboxLogs>,
    ) -> Result<State> {
        let logs = inbox_log.unwrap();
        let mut l = logs.lock();

        let PublishIdentityUpdateRequest { identity_update } = self;
    }
}
