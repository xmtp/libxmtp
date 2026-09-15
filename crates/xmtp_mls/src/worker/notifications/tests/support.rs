use crate::{context::XmtpSharedContext, tester, worker::WorkerConfig};
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Notify;
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

thread_local! {
    static BEFORE_REQUEST: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(crate) struct BeforeRequestGuard(std::marker::PhantomData<std::rc::Rc<()>>);

impl Drop for BeforeRequestGuard {
    fn drop(&mut self) {
        BEFORE_REQUEST.with(|hook| hook.borrow_mut().take());
    }
}

/// Run one local change after batch preparation and before request validation.
pub(crate) fn on_before_request(change: impl FnOnce() + 'static) -> BeforeRequestGuard {
    BEFORE_REQUEST.with(|hook| {
        assert!(hook.borrow_mut().replace(Box::new(change)).is_none());
    });
    BeforeRequestGuard(Default::default())
}

pub(crate) fn before_request() {
    let change = BEFORE_REQUEST.with(|hook| hook.borrow_mut().take());
    if let Some(change) = change {
        change();
    }
}

pub(crate) const TEST_TTL_NS: i64 = 4 * xmtp_common::NS_IN_DAY;
pub(crate) type Context = Arc<
    crate::context::XmtpMlsLocalContext<
        ScriptedApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Register,
    Unregister,
    Update,
    Publish,
}

#[derive(Default)]
pub(crate) struct Server {
    pub(crate) registered: bool,
    pub(crate) delivery: Option<wire::register_request::Delivery>,
    pub(crate) metadata: Vec<u8>,
    pub(crate) batches: Vec<(usize, usize)>,
    pub(crate) subscriptions: BTreeMap<Vec<u8>, wire::Subscription>,
    pub(crate) extra: u64,
    pub(crate) limit: Option<usize>,
    pub(crate) calls: Vec<Call>,
    pub(crate) pause_next: bool,
    pub(crate) next_error: Option<tonic::Code>,
}

#[derive(Default)]
pub(crate) struct Peer {
    pub(crate) state: Mutex<Server>,
    pub(crate) entered: Notify,
    pub(crate) release: Notify,
}

fn status(code: tonic::Code) -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        code,
        "notification test response",
    )))
}

impl Peer {
    async fn gate(&self, call: Call) -> Result<(), ApiClientError> {
        let (pause, error) = {
            let mut state = self.state.lock();
            state.calls.push(call);
            let pause = std::mem::take(&mut state.pause_next);
            (pause, state.next_error.take())
        };
        if pause {
            self.entered.notify_one();
            self.release.notified().await;
        }
        error.map(status).map_or(Ok(()), Err)
    }

    pub(crate) fn response(&self) -> wire::RecipientState {
        let state = self.state.lock();
        wire::RecipientState {
            topic_count: state.subscriptions.len() as u64 + state.extra,
            channel: match state.delivery {
                Some(wire::register_request::Delivery::Apns(_)) => wire::Channel::Apns,
                Some(wire::register_request::Delivery::Fcm(_)) => wire::Channel::Fcm,
                _ => wire::Channel::Http,
            } as i32,
            expires_at_ns: xmtp_common::time::now_ns() + TEST_TTL_NS,
        }
    }

    pub(crate) fn calls(&self, call: Call) -> usize {
        self.state
            .lock()
            .calls
            .iter()
            .filter(|seen| **seen == call)
            .count()
    }
}

#[derive(Clone)]
pub(crate) struct ScriptedApi {
    inner: crate::utils::TestClient,
    pub(crate) peer: Arc<Peer>,
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.peer.state.lock().calls.push(Call::Publish);
        self.inner.publish(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.inner.query(request).await
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }

    async fn register(
        &self,
        request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.peer.gate(Call::Register).await?;
        {
            let mut state = self.peer.state.lock();
            state.registered = true;
            state.delivery = request.delivery;
            state.metadata = request.metadata;
        }
        Ok(self.peer.response())
    }

    async fn unregister(
        &self,
        _: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.peer.gate(Call::Unregister).await?;
        let mut state = self.peer.state.lock();
        if !state.registered {
            return Err(status(tonic::Code::NotFound));
        }
        state.registered = false;
        state.subscriptions.clear();
        Ok(wire::UnregisterResponse {})
    }

    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.peer.gate(Call::Update).await?;
        {
            let mut state = self.peer.state.lock();
            state
                .batches
                .push((request.adds.len(), request.removes.len()));
            if !state.registered {
                return Err(status(tonic::Code::NotFound));
            }
            let mut next = state.subscriptions.clone();
            for topic in request.removes {
                next.remove(&topic);
            }
            for subscription in request.adds {
                next.insert(subscription.topic.clone(), subscription);
            }
            if state.limit.is_some_and(|limit| next.len() > limit) {
                return Err(status(tonic::Code::ResourceExhausted));
            }
            state.subscriptions = next;
        }
        Ok(self.peer.response())
    }
}

/// Reuse the normal registered tester, then replace only its notification responses.
pub(crate) async fn client() -> (crate::Client<Context>, Arc<Peer>) {
    tester!(base, disable_workers);
    let peer = Arc::new(Peer::default());
    let api = ScriptedApi {
        inner: base.context.api().api_client.clone(),
        peer: peer.clone(),
    };
    let client = crate::builder::ClientBuilder::from_client(base.client.clone())
        .api_client(api)
        .with_disable_workers(false)
        .worker_config(WorkerConfig::default())
        .with_allow_offline(Some(true))
        .build()
        .await
        .unwrap();
    client.workers.shutdown().await;
    (client, peer)
}
