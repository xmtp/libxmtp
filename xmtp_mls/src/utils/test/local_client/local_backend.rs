//! The naive 'local' backend

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
            tracing::debug!("committed message {cursor}");
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
            tracing::debug!("committed welcome {cursor}");
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

impl LocalTestClient {
    pub(super) fn query_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response> {
        let GetIdentityUpdatesV2Request { requests } = request;
        use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request;
        use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;
        use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::Response;
        let mut ret = Vec::new();

        let logs = self.identity_logs.read();
        for Request {
            inbox_id,
            sequence_id,
        } in requests.into_iter()
        {
            if let Some(log) = logs.inbox_logs.get(&hex::decode(&inbox_id)?) {
                let mut log = log.clone();
                log.sort_by_key(|k| k.sequence_id);
                let index = log
                    .binary_search_by_key(&sequence_id, |k| k.sequence_id as u64)
                    .unwrap_or(0);
                let updates = log[index..]
                    .iter()
                    .cloned()
                    .map(|log| IdentityUpdateLog {
                        sequence_id: log.sequence_id as u64,
                        server_timestamp_ns: log.server_timestamp_ns as u64,
                        update: Some(log.identity_update),
                    })
                    .collect();
                ret.push(Response {
                    inbox_id: inbox_id.clone(),
                    updates,
                })
            }
        }
        Ok(GetIdentityUpdatesV2Response { responses: ret })
    }

    pub(super) fn query_get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse> {
        use xmtp_proto::xmtp::identity::api::v1::get_inbox_ids_request::Request;
        use xmtp_proto::xmtp::identity::api::v1::get_inbox_ids_response::Response;

        let logs = self.identity_logs.read();
        let mut ret = Vec::new();
        for Request {
            address: target_address,
        } in request.requests.into_iter()
        {
            let mut address_logs: Vec<AddressLog> = logs
                .address_logs
                .iter()
                .filter(|((_, addr), _)| *addr == target_address)
                .map(|(_, logs)| logs)
                .cloned()
                .flatten()
                .filter(|l| l.revocation_sequence_id.is_none())
                .collect();
            address_logs.sort_by_key(|k| k.association_sequence_id);
            // get maximum
            address_logs.reverse();
            address_logs.dedup_by_key(|k| k.address.clone());

            let logs: Vec<_> = address_logs
                .into_iter()
                .map(|l| Response {
                    address: l.address,
                    inbox_id: Some(hex::encode(l.inbox_id)),
                })
                .collect();
            ret.extend(logs);
        }
        Ok(GetInboxIdsResponse { responses: ret })
    }

    pub(super) fn query_group_messages_local(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse> {
        let QueryGroupMessagesRequest {
            group_id,
            paging_info,
        } = request;
        let mut messages = self.group_messages(group_id);
        let mut limit = 100; // 100 is the max limit
        let mut paging_info_out = PagingInfo::default();
        let mut id_cursor = 0;
        // default descending
        messages.sort_by_key(|m| std::cmp::Reverse(m.id));
        tracing::debug!("total messages {}", get_id_list(messages.as_slice()));

        if let Some(info) = paging_info {
            id_cursor = info.id_cursor;
            tracing::debug!("queried from id: {id_cursor} direction {}", info.direction);

            limit = std::cmp::min(info.limit, limit);
            match info.direction {
                0 => paging_info_out.direction = 0,
                1 => {
                    // Ascending
                    paging_info_out.direction = 1;
                    messages.reverse()
                }
                2 => paging_info_out.direction = 2, // Descending
                _ => unreachable!(),
            };
        }

        if id_cursor > 0 {
            messages.retain(|m| m.id > id_cursor as usize);
            if messages.len() > 0 {
                let limit = std::cmp::min(messages.len(), limit as usize);
                let _ = messages.split_off(limit);
            }
        } else {
            let limit = std::cmp::min(messages.len(), limit as usize);
            messages = messages[0..limit as usize].to_vec();
        }

        if messages.len() >= limit as usize {
            if let Some(last) = messages.last() {
                paging_info_out.id_cursor = last.id as u64;
            }
        }
        tracing::debug!(
            "returning messages with ids: [{}]",
            get_id_list(messages.as_slice())
        );

        Ok(QueryGroupMessagesResponse {
            messages: messages.into_iter().map(Into::into).collect(),
            paging_info: Some(paging_info_out),
        })
    }

    pub(super) fn query_welcome_messages_local(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse> {
        let QueryWelcomeMessagesRequest {
            installation_key,
            paging_info,
        } = request;

        let mut messages = self.welcome_messages(installation_key);
        let mut limit = 100; // 100 is the max limit
        let mut paging_info_out = PagingInfo::default();
        let mut id_cursor = 0;
        // default descending
        messages.sort_by_key(|m| std::cmp::Reverse(m.id));
        tracing::debug!("total welcomes {}", get_id_list(messages.as_slice()));

        if let Some(info) = paging_info {
            id_cursor = info.id_cursor;
            tracing::debug!("queried from id: {id_cursor} direction {}", info.direction);

            limit = std::cmp::min(info.limit, limit);
            match info.direction {
                0 => paging_info_out.direction = 0,
                1 => {
                    // ascending
                    paging_info_out.direction = 1;
                    messages.reverse()
                }
                2 => paging_info_out.direction = 2, // Descending
                _ => unreachable!(),
            };
        }

        if id_cursor > 0 {
            messages.retain(|m| m.id > id_cursor as usize);
            if messages.len() > 0 {
                let limit = std::cmp::min(messages.len(), limit as usize);
                let _ = messages.split_off(limit);
            }
        } else {
            let limit = std::cmp::min(messages.len(), limit as usize);
            messages = messages[0..limit].to_vec();
        }

        if messages.len() >= limit as usize {
            if let Some(last) = messages.last() {
                paging_info_out.id_cursor = last.id as u64;
            }
        }

        tracing::debug!(
            "returning welcomes with id [{}]",
            get_id_list(messages.as_slice())
        );
        Ok(QueryWelcomeMessagesResponse {
            messages: messages.into_iter().map(Into::into).collect(),
            paging_info: Some(paging_info_out),
        })
    }

    pub(super) fn fetch_key_packages_local(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse> {
        let mut ret = Vec::new();
        for key in request.installation_keys.iter() {
            let kp = self.key_package(key.clone());
            ret.push(fetch_key_packages_response::KeyPackage {
                key_package_tls_serialized: kp.inner.tls_serialize_detached()?,
            })
        }
        Ok(FetchKeyPackagesResponse { key_packages: ret })
    }
}
