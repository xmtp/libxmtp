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
use futures::{Stream, StreamExt};
use openmls::prelude::ProtocolMessage;
use parking_lot::{Mutex, RwLock};
use tls_codec::Serialize;
use tokio::sync::broadcast;
use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient},
    identity::api::v1::prelude::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, *,
    },
    mls::api::v1::prelude::*,
};
use xmtp_proto::{
    api_client::{XmtpMlsStreams, XmtpTestClient},
    xmtp::identity::associations::IdentityUpdate,
};

mod error;
mod process;

type Result<T> = std::result::Result<T, error::LocalClientError>;

trait ProcessLocalRequest {
    fn process(
        &self,
        cursor: Option<&AtomicUsize>,
        inbox_log: Option<Arc<RwLock<IdentityLogs>>>,
    ) -> Result<State>;
}

#[derive(Debug, Clone)]
pub struct LocalTestClient {
    state: Arc<Mutex<State>>,
    identity_logs: Arc<RwLock<IdentityLogs>>,
    message_cursor: Arc<AtomicUsize>,
    welcome_cursor: Arc<AtomicUsize>,
    sequence_id: Arc<AtomicUsize>,
    tx: broadcast::Sender<WelcomeOrMessage>,
}

#[derive(Clone, Debug)]
enum WelcomeOrMessage {
    Message(Vec<LocalGroupMessage>),
    Welcome(Vec<LocalWelcomeMessage>),
}

impl Default for LocalTestClient {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            state: Default::default(),
            identity_logs: Default::default(),
            message_cursor: Arc::new(AtomicUsize::new(1)),
            welcome_cursor: Arc::new(AtomicUsize::new(1)),
            sequence_id: Arc::new(AtomicUsize::new(1)),
            tx,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct IdentityLogs {
    inbox_logs: HashMap<Vec<u8>, Vec<InboxLog>>,
    // InboxId to Address
    address_logs: HashMap<(Vec<u8>, String), Vec<AddressLog>>,
}

#[derive(Clone, Default, Debug)]
struct InboxLog {
    sequence_id: usize,
    inbox_id: Vec<u8>,
    identity_update: IdentityUpdate,
    server_timestamp_ns: i64,
}

#[derive(Clone, Default, Debug)]
struct AddressLog {
    address: String,
    inbox_id: Vec<u8>,
    association_sequence_id: usize,
    revocation_sequence_id: Option<usize>,
}
#[derive(Clone, Default, Debug)]
pub struct State {
    messages: HashMap<GroupId, Vec<LocalGroupMessage>>,
    welcomes: HashMap<InstallationId, Vec<LocalWelcomeMessage>>,
    key_packages: HashMap<InstallationId, VerifiedKeyPackageV2>,
}

trait Id {
    fn id(&self) -> usize;
}

impl Id for LocalGroupMessage {
    fn id(&self) -> usize {
        self.id
    }
}

impl Id for LocalWelcomeMessage {
    fn id(&self) -> usize {
        self.id
    }
}

fn get_id_list(items: &[impl Id]) -> String {
    let mut s = String::new();
    for item in items {
        s += &(item.id().to_string() + ",");
    }
    s
}

impl State {
    fn apply(&mut self, diff: State) {
        for (id, msgs) in diff.messages.into_iter() {
            self.messages
                .entry(id)
                .or_insert_with(Vec::new)
                .extend(msgs);
        }
        for (id, welcomes) in diff.welcomes.into_iter() {
            self.welcomes
                .entry(id)
                .or_insert_with(Vec::new)
                .extend(welcomes)
        }
        self.key_packages.extend(diff.key_packages.into_iter());
    }
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

    fn notify_subscriptions(&self, diff: &State) {
        for message in diff.messages.values().cloned() {
            let _ = self.tx.send(WelcomeOrMessage::Message(message));
        }
        for welcome in diff.welcomes.values().cloned() {
            let _ = self.tx.send(WelcomeOrMessage::Welcome(welcome));
        }
    }

    fn key_package<T: TryInto<InstallationId>>(&self, id: T) -> VerifiedKeyPackageV2
    where
        <T as TryInto<InstallationId>>::Error: std::fmt::Debug,
    {
        let this = self.state.lock();
        this.key_packages
            .get(&id.try_into().unwrap())
            .unwrap()
            .clone()
    }

    fn group_messages<T: Into<GroupId>>(&self, id: T) -> Vec<LocalGroupMessage> {
        let this = self.state.lock();
        if let Some(m) = this.messages.get(&id.into()) {
            m.clone()
        } else {
            Vec::new()
        }
    }

    fn welcome_messages<T: TryInto<InstallationId>>(&self, id: T) -> Vec<LocalWelcomeMessage>
    where
        <T as TryInto<InstallationId>>::Error: std::fmt::Debug,
    {
        let this = self.state.lock();
        if let Some(m) = this.welcomes.get(&id.try_into().unwrap()) {
            m.clone()
        } else {
            Vec::new()
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsClient for LocalTestClient {
    type Error = error::LocalClientError;

    #[tracing::instrument(level = "info")]
    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<()> {
        self.apply(request.process(None, None)?);
        Ok(())
    }

    #[tracing::instrument(level = "info")]
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

    #[tracing::instrument(level = "info")]
    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<()> {
        let diff = request.process(Some(&self.message_cursor), None)?;
        self.notify_subscriptions(&diff);
        self.apply(diff);
        Ok(())
    }

    #[tracing::instrument(level = "info")]
    async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<()> {
        let diff = request.process(Some(&self.welcome_cursor), None)?;
        self.notify_subscriptions(&diff);
        self.apply(diff);
        Ok(())
    }

    #[tracing::instrument(level = "info")]
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
        let mut id_cursor = 0;
        // default descending
        messages.sort_by_key(|m| std::cmp::Reverse(m.id));
        tracing::info!("total messages {}", get_id_list(messages.as_slice()));

        if let Some(info) = paging_info {
            id_cursor = info.id_cursor;
            tracing::info!("queried from id: {id_cursor} direction {}", info.direction);

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
        tracing::info!(
            "returning messages with ids: [{}]",
            get_id_list(messages.as_slice())
        );

        Ok(QueryGroupMessagesResponse {
            messages: messages.into_iter().map(Into::into).collect(),
            paging_info: Some(paging_info_out),
        })
    }

    #[tracing::instrument(level = "info")]
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
        let mut id_cursor = 0;
        // default descending
        messages.sort_by_key(|m| std::cmp::Reverse(m.id));
        tracing::info!("total welcomes {}", get_id_list(messages.as_slice()));

        if let Some(info) = paging_info {
            id_cursor = info.id_cursor;
            tracing::info!("queried from id: {id_cursor} direction {}", info.direction);

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

        tracing::info!(
            "returning welcomes with id [{}]",
            get_id_list(messages.as_slice())
        );
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

    #[tracing::instrument(level = "info")]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse> {
        let logs =
            self.apply(request.process(Some(&self.sequence_id), Some(self.identity_logs.clone()))?);

        Ok(PublishIdentityUpdateResponse {})
    }

    #[tracing::instrument(level = "info")]
    async fn get_identity_updates_v2(
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

    // recent active (non-revoked) assciation for each address in a provided list.
    #[tracing::instrument(level = "info")]
    async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse> {
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

    #[tracing::instrument(level = "info")]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse> {
        use xmtp_proto::xmtp::identity::api::v1::verify_smart_contract_wallet_signatures_response::ValidationResponse;
        let len = request.signatures.len();
        let mut ret = Vec::with_capacity(len);
        for _ in 0..len {
            ret.push(ValidationResponse {
                is_valid: true,
                block_number: Some(0),
                error: None,
            })
        }

        Ok(VerifySmartContractWalletSignaturesResponse { responses: ret })
    }
}

#[async_trait::async_trait]
impl XmtpMlsStreams for LocalTestClient {
    type GroupMessageStream<'a> =
        Box<dyn Stream<Item = Result<GroupMessage>> + Unpin + Send + Sync>;
    type WelcomeMessageStream<'a> =
        Box<dyn Stream<Item = Result<WelcomeMessage>> + Unpin + Send + Sync>;
    type Error = error::LocalClientError;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>> {
        use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
        use xmtp_proto::xmtp::mls::api::v1::group_message::V1;

        let receiver = self.tx.subscribe();
        let s = tokio_stream::wrappers::BroadcastStream::new(receiver);
        s.filter_map(|m| match m {
            Ok(WelcomeOrMessage::Message(msgs)) => msgs.into_iter().map(|m| GroupMessage {
                version: Some(Version::V1(V1 {
                    id: m.id as u64,
                    created_ns: m.created as u64,
                    group_id: m.msg.group_id().to_vec(),
                    data: m.data,
                    sender_hmac: m.sender_hmac,
                })),
            }),
            _ => None,
        })
        .boxed()
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>> {
        todo!()
    }
}

use std::sync::LazyLock;
static GLOBAL_LOCAL_CLIENT: LazyLock<LocalTestClient> = LazyLock::new(|| LocalTestClient::new());

#[async_trait::async_trait]
impl XmtpTestClient for LocalTestClient {
    async fn create_local() -> Self {
        GLOBAL_LOCAL_CLIENT.clone()
    }
    async fn create_dev() -> Self {
        GLOBAL_LOCAL_CLIENT.clone()
    }
}
