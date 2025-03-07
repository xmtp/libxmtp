//! A local client that transparently feeds data from 'push' calls to 'pull' calls
//! A very naive implementation of what xmtp-node-go is doing, but which lets us manipulate the
//! calls
//! to create malformed or malicious queries for unit tests.
//! also allows us to run tests without any network connection.

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{atomic::AtomicUsize, Arc},
};

use crate::{
    types::{GroupId, InstallationId},
    verified_key_package_v2::VerifiedKeyPackageV2,
};
use error::LocalClientError;
use futures::{Stream, StreamExt};
use openmls::prelude::ProtocolMessage;
use parking_lot::{Mutex, RwLock};
use tls_codec::Serialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
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
mod local_backend;
mod modification;
use modification::ModificationType;

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
    modification: Arc<Mutex<HashMap<&'static str, Vec<ModificationType>>>>,
}

#[derive(Clone, Debug)]
enum WelcomeOrMessage {
    Message(LocalGroupMessage),
    Welcome(LocalWelcomeMessage),
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
            modification: Arc::new(Mutex::new(HashMap::new())),
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
    #[allow(unused)]
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
    #[allow(unused)]
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
        for msgs in diff.messages.values().cloned() {
            tracing::info!("NOTIFYING message: {}", get_id_list(msgs.as_slice()));

            msgs.into_iter().for_each(|m| {
                let _ = self.tx.send(WelcomeOrMessage::Message(m));
            });
        }
        for welcomes in diff.welcomes.values().cloned() {
            tracing::info!("NOTIFYING welcome: {}", get_id_list(welcomes.as_slice()));
            welcomes.into_iter().for_each(|w| {
                let _ = self.tx.send(WelcomeOrMessage::Welcome(w));
            });
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
        let mut response = self.fetch_key_packages_local(request)?;
        if let Some(m) = self.get_mod(std::any::type_name::<FetchKeyPackagesResponse>()) {
            let f = m.fetch_kps();
            f(&mut response)
        }
        Ok(response)
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
        let mut response = self.query_group_messages_local(request)?;
        if let Some(m) = self.get_mod(std::any::type_name::<QueryGroupMessagesResponse>()) {
            let f = m.query_group_messages();
            f(&mut response)
        }
        Ok(response)
    }

    #[tracing::instrument(level = "info")]
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse> {
        let mut response = self.query_welcome_messages_local(request)?;
        if let Some(m) = self.get_mod(std::any::type_name::<QueryWelcomeMessagesResponse>()) {
            let f = m.query_welcome_messages();
            f(&mut response)
        }
        Ok(response)
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
        self.apply(request.process(Some(&self.sequence_id), Some(self.identity_logs.clone()))?);

        Ok(PublishIdentityUpdateResponse {})
    }

    #[tracing::instrument(level = "info")]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response> {
        let mut response = self.query_identity_updates_v2(request)?;
        if let Some(m) = self.get_mod(std::any::type_name::<GetIdentityUpdatesV2Response>()) {
            let f = m.get_identity_updates();
            f(&mut response)
        }

        Ok(response)
    }

    // recent active (non-revoked) assciation for each address in a provided list.
    #[tracing::instrument(level = "info")]
    async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse> {
        let mut response = self.query_get_inbox_ids(request)?;
        if let Some(m) = self.get_mod(std::any::type_name::<GetInboxIdsResponse>()) {
            let f = m.get_inbox_ids();
            f(&mut response)
        }
        Ok(response)
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
        let mut response = VerifySmartContractWalletSignaturesResponse { responses: ret };
        if let Some(m) = self.get_mod(std::any::type_name::<
            VerifySmartContractWalletSignaturesResponse,
        >()) {
            let f = m.verify_scw_signatures();
            f(&mut response)
        }

        Ok(response)
    }
}

#[async_trait::async_trait]
impl XmtpMlsStreams for LocalTestClient {
    type GroupMessageStream<'a> = Pin<Box<dyn Stream<Item = Result<GroupMessage>> + Send>>;
    type WelcomeMessageStream<'a> = Pin<Box<dyn Stream<Item = Result<WelcomeMessage>> + Send>>;
    type Error = error::LocalClientError;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>> {
        let receiver = self.tx.subscribe();
        let s = tokio_stream::wrappers::BroadcastStream::new(receiver);
        let mut historic_messages = Vec::new();
        let SubscribeGroupMessagesRequest { ref filters } = request;
        for filter in filters {
            let mut msgs = self.group_messages(filter.group_id.clone());
            msgs.reverse();
            historic_messages.extend(msgs.into_iter().map(|m| WelcomeOrMessage::Message(m)));
        }
        let r = request.clone();
        let historic_stream = futures::stream::iter(historic_messages).filter_map(move |m| {
            let item = filter_message(r.clone(), Ok(m));
            futures::future::ready(item)
        });
        let broadcast_stream = s.filter_map(move |m| {
            let item = filter_message(request.clone(), m);
            futures::future::ready(item)
        });

        let this = self.clone();
        Ok(historic_stream
            .chain(broadcast_stream)
            .map(move |msg| {
                let mut msg = msg?;
                if let Some(m) = this.get_mod(std::any::type_name::<GroupMessage>()) {
                    let f = m.next_streamed_message();
                    f(&mut msg)
                }
                Ok(msg.clone())
            })
            .boxed())
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>> {
        let receiver = self.tx.subscribe();
        let s = tokio_stream::wrappers::BroadcastStream::new(receiver);
        let SubscribeWelcomeMessagesRequest { ref filters } = request;
        let mut historic_messages = Vec::new();

        for filter in filters {
            let mut msgs = self.welcome_messages(filter.installation_key.clone());
            msgs.reverse();
            historic_messages.extend(msgs.into_iter().map(|w| WelcomeOrMessage::Welcome(w)));
        }
        let r = request.clone();
        let historic_stream = futures::stream::iter(historic_messages).filter_map(move |m| {
            let item = filter_welcome(r.clone(), Ok(m));
            futures::future::ready(item)
        });

        let broadcast_stream = s
            .filter_map(move |m| {
                let item = filter_welcome(request.clone(), m);
                futures::future::ready(item)
            })
            .boxed();

        let this = self.clone();
        Ok(historic_stream
            .chain(broadcast_stream)
            .map(move |msg| {
                let mut msg = msg?;
                if let Some(m) = this.get_mod(std::any::type_name::<WelcomeMessage>()) {
                    let f = m.next_streamed_welcome();
                    f(&mut msg)
                }
                Ok(msg.clone())
            })
            .boxed())
    }
}

fn filter_message(
    request: SubscribeGroupMessagesRequest,
    msg: std::result::Result<WelcomeOrMessage, BroadcastStreamRecvError>,
) -> Option<Result<GroupMessage>> {
    use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
    use xmtp_proto::xmtp::mls::api::v1::group_message::V1;

    match msg {
        Ok(WelcomeOrMessage::Message(msg)) => {
            // ensure that passes filter
            if !request
                .filters
                .iter()
                .any(|f| msg.msg.group_id().to_vec() == f.group_id && msg.id > f.id_cursor as usize)
            {
                return None;
            }
            Some(Ok::<_, LocalClientError>(GroupMessage {
                version: Some(Version::V1(V1 {
                    id: msg.id as u64,
                    created_ns: msg.created as u64,
                    group_id: msg.msg.group_id().to_vec(),
                    data: msg.data,
                    sender_hmac: msg.sender_hmac,
                })),
            }))
        }
        _ => None,
    }
}

fn filter_welcome(
    request: SubscribeWelcomeMessagesRequest,
    msg: std::result::Result<WelcomeOrMessage, BroadcastStreamRecvError>,
) -> Option<Result<WelcomeMessage>> {
    use xmtp_proto::xmtp::mls::api::v1::welcome_message::Version;
    use xmtp_proto::xmtp::mls::api::v1::welcome_message::V1;

    match msg {
        Ok(WelcomeOrMessage::Welcome(w)) => {
            // ensure that passes filter
            if !request
                .filters
                .iter()
                .any(|f| w.installation_key == f.installation_key && w.id > f.id_cursor as usize)
            {
                return None;
            }
            Some(Ok::<_, LocalClientError>(WelcomeMessage {
                version: Some(Version::V1(V1 {
                    id: w.id as u64,
                    created_ns: w.created_at as u64,
                    installation_key: w.installation_key,
                    data: w.data,
                    hpke_public_key: w.hpke_public_key,
                })),
            }))
        }
        _ => None,
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
