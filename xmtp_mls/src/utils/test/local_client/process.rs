use super::*;
use crate::{types::InstallationId, verified_key_package_v2::VerifiedKeyPackageV2};
use futures_util::FutureExt;
use openmls::prelude::{MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use std::sync::atomic::Ordering;
use tls_codec::Deserialize;
use xmtp_id::associations::{get_association_state, MemberIdentifier};

impl ProcessLocalRequest for UploadKeyPackageRequest {
    #[tracing::instrument(level = "info")]
    fn process(
        &self,
        _cursor: Option<&AtomicUsize>,
        _inbox_log: Option<Arc<RwLock<IdentityLogs>>>,
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
    #[tracing::instrument(level = "info")]
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        _inbox_log: Option<Arc<RwLock<IdentityLogs>>>,
    ) -> Result<State> {
        use xmtp_proto::xmtp::mls::api::v1::group_message_input::Version;
        use xmtp_proto::xmtp::mls::api::v1::group_message_input::V1;
        let cursor = cursor.unwrap();
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
            let cursor = cursor.fetch_add(1, Ordering::SeqCst);
            let msg_result = MlsMessageIn::tls_deserialize(&mut data.as_slice())?;
            let protocol_message: ProtocolMessage = msg_result.try_into_protocol_message()?;
            ret.entry(protocol_message.group_id().clone().to_vec().into())
                .or_insert_with(Vec::new)
                .push(LocalGroupMessage {
                    id: cursor,
                    msg: protocol_message,
                    data: data.clone(),
                    sender_hmac: sender_hmac.to_vec(),
                    created: xmtp_common::time::now_ns(),
                });
            tracing::info!("committed message {cursor}");
        }

        Ok(State {
            messages: ret,
            ..Default::default()
        })
    }
}

// id, created_at, installation_key, data, hpke_public_key, installation_key_data_hash
impl ProcessLocalRequest for SendWelcomeMessagesRequest {
    #[tracing::instrument(level = "info")]
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        _inbox_log: Option<Arc<RwLock<IdentityLogs>>>,
    ) -> Result<State> {
        use xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version;
        use xmtp_proto::xmtp::mls::api::v1::welcome_message_input::V1;
        let cursor = cursor.unwrap();

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
            let cursor = cursor.fetch_add(1, Ordering::SeqCst);
            tracing::info!("committed welcome {cursor}");
            ret.entry(installation_key.to_vec().try_into().unwrap())
                .or_insert_with(Vec::new)
                .push(LocalWelcomeMessage {
                    id: cursor,
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
    #[tracing::instrument(level = "info")]
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        identity_logs: Option<Arc<RwLock<IdentityLogs>>>,
    ) -> Result<State> {
        use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

        let logs = identity_logs.unwrap();
        let mut l = logs.write();
        let IdentityLogs {
            ref mut inbox_logs,
            ref mut address_logs,
        } = *l;

        let PublishIdentityUpdateRequest { identity_update } = self;

        if identity_update.is_none() {
            return Ok(State::default());
        }
        let sequence_id = cursor.unwrap();
        let identity_update = identity_update.clone().unwrap();
        let inbox_id_bytes = hex::decode(&identity_update.inbox_id)?;

        let updates = inbox_logs
            .get(&inbox_id_bytes)
            .map(|log| log.iter().map(|l| l.identity_update.clone()).collect())
            .unwrap_or(Vec::new());

        let state = get_association_state(
            updates,
            vec![identity_update.clone()],
            MockSmartContractSignatureVerifier::new(true),
        )
        .now_or_never()
        .expect("mock verifier must resolve immediately since no async action occurs")?;
        let sequence_id = sequence_id.fetch_add(1, Ordering::SeqCst);

        inbox_logs
            .entry(inbox_id_bytes.clone())
            .or_insert_with(Vec::new)
            .push(InboxLog {
                sequence_id,
                inbox_id: inbox_id_bytes.clone(),
                identity_update,
                server_timestamp_ns: xmtp_common::time::now_ns(),
            });

        for new_member in state.state_diff.new_members.iter() {
            match new_member {
                MemberIdentifier::Address(addr) => address_logs
                    .entry((inbox_id_bytes.clone(), addr.clone()))
                    .or_insert_with(Vec::new)
                    .push(AddressLog {
                        association_sequence_id: sequence_id,
                        revocation_sequence_id: None,
                        inbox_id: inbox_id_bytes.clone(),
                        address: addr.clone(),
                    }),
                _ => continue,
            }
        }

        for removed_member in state.state_diff.removed_members.iter() {
            match removed_member {
                MemberIdentifier::Address(addr) => {
                    let logs = address_logs.get_mut(&(inbox_id_bytes.clone(), addr.clone()));
                    if let Some(log_for_id_addr) = logs {
                        log_for_id_addr.sort_by_key(|k| k.association_sequence_id);
                        if let Some(max) = log_for_id_addr.last_mut() {
                            max.revocation_sequence_id = Some(sequence_id);
                        }
                    }
                }
                _ => continue,
            }
        }

        Ok(State::default())
    }
}
