//! A local client that transparently feeds data from 'push' calls to 'pull' calls
//! A very naive implementation of what xmtp-node-go is doing, but which lets us manipulate the
//! calls
//! to create malformed or malicious queries for unit tests.
//! also allows us to run tests without any network connection.

use std::{
    collections::HashMap,
    sync::{atomic::AtomicUsize, Arc},
};

use crate::{
    types::{GroupId, InstallationId},
    verified_key_package_v2::VerifiedKeyPackageV2,
};
use openmls::prelude::ProtocolMessage;
use parking_lot::Mutex;
use tls_codec::Serialize;
use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient},
    identity::api::v1::prelude::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, *,
    },
    mls::api::v1::prelude::*,
};

mod error;
mod process;

type Result<T> = std::result::Result<T, error::LocalClientError>;

type LocalInboxLogs = Arc<Mutex<HashMap<Vec<u8>, Vec<InboxLog>>>>;
trait ProcessLocalRequest {
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        inbox_log: Option<LocalInboxLogs>,
    ) -> Result<State>;
}

#[derive(Default, Debug, Clone)]
pub struct LocalTestClient {
    state: Arc<Mutex<State>>,
    message_cursor: Arc<AtomicUsize>,
    welcome_cursor: Arc<AtomicUsize>,
    inbox_log: LocalInboxLogs,
}

#[derive(Clone, Debug)]
struct LocalGroupMessage {
    id: usize,
    msg: ProtocolMessage,
    data: Vec<u8>,
    sender_hmac: Vec<u8>,
    created: i64,
}

#[derive(Debug, Clone)]
struct LocalWelcomeMessage {
    id: usize,
    created_at: i64,
    installation_key: Vec<u8>,
    data: Vec<u8>,
    hpke_public_key: Vec<u8>,
    installation_key_data_hash: Vec<u8>,
}

impl From<LocalGroupMessage> for GroupMessage {
    fn from(local: LocalGroupMessage) -> GroupMessage {
        use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
        use xmtp_proto::xmtp::mls::api::v1::group_message::V1;

        GroupMessage {
            version: Some(Version::V1(V1 {
                id: local.id as u64,
                created_ns: local.created as u64,
                group_id: local.msg.group_id().to_vec(),
                data: local.data,
                sender_hmac: local.sender_hmac,
            })),
        }
    }
}

impl From<LocalWelcomeMessage> for WelcomeMessage {
    fn from(local: LocalWelcomeMessage) -> WelcomeMessage {
        use xmtp_proto::xmtp::mls::api::v1::welcome_message::Version;
        use xmtp_proto::xmtp::mls::api::v1::welcome_message::V1;

        WelcomeMessage {
            version: Some(Version::V1(V1 {
                id: local.id as u64,
                created_ns: local.created_at as u64,
                installation_key: local.installation_key,
                data: local.data,
                hpke_public_key: local.hpke_public_key,
            })),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct State {
    messages: HashMap<GroupId, Vec<LocalGroupMessage>>,
    welcomes: HashMap<InstallationId, Vec<LocalWelcomeMessage>>,
    key_packages: HashMap<InstallationId, VerifiedKeyPackageV2>,
}

#[derive(Clone, Default, Debug)]
pub struct InboxLog {
    sequence_id: usize,
    inbox_id: Vec<u8>,
    identity_update: xmtp_proto::xmtp::identity::associations::IdentityUpdate,
}

impl State {
    fn apply(&mut self, diff: State) {
        self.messages.extend(diff.messages.into_iter());
        self.welcomes.extend(diff.welcomes.into_iter());
        self.key_packages.extend(diff.key_packages.into_iter());
    }
}

impl LocalTestClient {
    pub fn new() -> Self {
        // if we send 64 messages to a channel
        // and don't process them fast enouhg
        // we got a bigger problem somewhere
        // let (tx, rx) = channel(64);
        Self {
            ..Default::default()
        }
    }

    fn apply(&self, diff: State) {
        let mut s = self.state.lock();
        s.apply(diff)
    }

    fn key_package<T: TryInto<InstallationId>>(&self, id: T) -> VerifiedKeyPackageV2
    where
        <T as TryInto<InstallationId>>::Error: std::fmt::Debug,
    {
        let mut this = self.state.lock();
        this.key_packages
            .get(&id.try_into().unwrap())
            .unwrap()
            .clone()
    }

    fn group_messages<T: Into<GroupId>>(&self, id: T) -> Vec<LocalGroupMessage> {
        let this = self.state.lock();
        this.messages.get(&id.into()).unwrap().clone()
    }

    fn welcome_messages<T: TryInto<InstallationId>>(&self, id: T) -> Vec<LocalWelcomeMessage>
    where
        <T as TryInto<InstallationId>>::Error: std::fmt::Debug,
    {
        let this = self.state.lock();
        this.welcomes.get(&id.try_into().unwrap()).unwrap().clone()
    }
    /*
    fn inbox_log(&self, id: String) -> Vec<InboxLog> {
        let this = self.state.lock();
        this.inbox_log
            .get(&hex::decode(&id).unwrap())
            .unwrap()
            .clone()
    }
    */
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsClient for LocalTestClient {
    type Error = error::LocalClientError;

    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<()> {
        self.apply(request.process(None, None)?);
        Ok(())
    }

    async fn fetch_key_packages(
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

    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<()> {
        self.apply(request.process(Some(&self.message_cursor), None)?);
        Ok(())
    }

    async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<()> {
        self.apply(request.process(Some(&self.welcome_cursor), None)?);
        Ok(())
    }

    async fn query_group_messages(
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

        if let Some(info) = paging_info {
            messages.sort_by_key(|m| m.id);
            match info.direction {
                0 => paging_info_out.direction = 0,
                1 => paging_info_out.direction = 1, // Ascending
                2 => {
                    paging_info_out.direction = 2;
                    messages.reverse()
                } // Descending
                _ => unreachable!(),
            };
            limit = std::cmp::min(info.limit, limit);
            if info.id_cursor > 0 {
                let from_item_index = messages
                    .binary_search_by_key(&(info.id_cursor as usize), |m| m.id)
                    .unwrap();
                messages = messages[from_item_index..from_item_index + (limit as usize)].to_vec();
            } else {
                messages = messages[0..limit as usize].to_vec();
            }
        }

        if messages.len() >= limit as usize {
            if let Some(last) = messages.last() {
                paging_info_out.id_cursor = last.id as u64;
            }
        }

        Ok(QueryGroupMessagesResponse {
            messages: messages.into_iter().map(Into::into).collect(),
            paging_info: Some(paging_info_out),
        })
    }

    async fn query_welcome_messages(
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

        if let Some(info) = paging_info {
            messages.sort_by_key(|m| m.id);
            match info.direction {
                0 => paging_info_out.direction = 0,
                1 => paging_info_out.direction = 1, // Ascending
                2 => {
                    paging_info_out.direction = 2;
                    messages.reverse()
                } // Descending
                _ => unreachable!(),
            };
            limit = std::cmp::min(info.limit, limit);
            if info.id_cursor > 0 {
                let from_item_index = messages
                    .binary_search_by_key(&(info.id_cursor as usize), |m| m.id)
                    .unwrap();
                messages = messages[from_item_index..from_item_index + (limit as usize)].to_vec();
            } else {
                messages = messages[0..limit as usize].to_vec();
            }
        }

        if messages.len() >= limit as usize {
            if let Some(last) = messages.last() {
                paging_info_out.id_cursor = last.id as u64;
            }
        }

        Ok(QueryWelcomeMessagesResponse {
            messages: messages.into_iter().map(Into::into).collect(),
            paging_info: Some(paging_info_out),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpIdentityClient for LocalTestClient {
    type Error = error::LocalClientError;
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse> {
        self.apply(request.process(None, None)?);
        Ok(PublishIdentityUpdateResponse {})
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response> {
        todo!()
    }

    async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse> {
        todo!()
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse> {
        todo!()
    }
}
