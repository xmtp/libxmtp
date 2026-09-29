//! A backend double that refuses chosen publishes or loses their responses.

use super::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

pub(super) type FaultyContext = Arc<
    crate::context::XmtpMlsLocalContext<
        FaultyApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// What the backend does with one intercepted publish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    /// Fail with this status before anything is stored.
    Refuse(tonic::Code),
    /// Store the request, then lose its response to `OUT_OF_RANGE`.
    LoseResponse,
}

/// Passes every request to the test backend, except that each publish carrying
/// an envelope `intercepts` selects takes the next queued fault, one at a time.
#[derive(Clone)]
pub(super) struct FaultyApi {
    inner: crate::utils::TestClient,
    intercepts: fn(&Payload) -> bool,
    faults: Arc<Mutex<VecDeque<Fault>>>,
    /// Every intercepted request, in the order sent.
    pub requests: Arc<Mutex<Vec<wire::PublishRequest>>>,
    /// Metadata of each stored request whose response was lost.
    pub lost: Arc<Mutex<Vec<wire::EnvelopeMeta>>>,
    /// Fail every `Query` after the next `OUT_OF_RANGE` with `DATA_LOSS` until
    /// [`Self::restart`], as if the process stopped before its recovery read.
    pub stop_before_recovery: Arc<AtomicBool>,
    poisoned: Arc<AtomicBool>,
    /// Intercepted publishes run one at a time, so the first to fail ends a
    /// concurrent batch before any other intercepted request is sent.
    serial: Arc<futures::lock::Mutex<()>>,
}

impl FaultyApi {
    pub(super) fn new(
        tester: &crate::utils::ClientTester,
        intercepts: fn(&Payload) -> bool,
        faults: impl IntoIterator<Item = Fault>,
    ) -> Self {
        Self {
            inner: tester.context.api().api_client.clone(),
            intercepts,
            faults: Arc::new(Mutex::new(faults.into_iter().collect())),
            requests: Default::default(),
            lost: Default::default(),
            stop_before_recovery: Default::default(),
            poisoned: Default::default(),
            serial: Default::default(),
        }
    }

    /// Let queries through again, as a restarted process would.
    pub(super) fn restart(&self) {
        self.poisoned.store(false, Ordering::SeqCst);
    }

    /// The intercepted requests that carried `envelope`.
    pub(super) fn sends_of(&self, envelope: &wire::ClientEnvelope) -> usize {
        self.requests
            .lock()
            .iter()
            .filter(|request| request.envelopes.contains(envelope))
            .count()
    }
}

pub(super) fn group_message(payload: &Payload) -> bool {
    matches!(payload, Payload::GroupMessage(_))
}

pub(super) fn welcome(payload: &Payload) -> bool {
    matches!(payload, Payload::WelcomeMessage(_))
}

fn status(code: tonic::Code) -> ApiClientError {
    ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(tonic::Status::new(
        code, "injected",
    )))
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for FaultyApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        let intercepted = request
            .envelopes
            .iter()
            .any(|envelope| envelope.payload.as_ref().is_some_and(self.intercepts));
        if !intercepted {
            return self.inner.publish(request).await;
        }
        let _serial = self.serial.lock().await;
        let fault = self.faults.lock().pop_front();
        self.requests.lock().push(request.clone());
        let out_of_range = match fault {
            None => return self.inner.publish(request).await,
            Some(Fault::Refuse(code)) if code != tonic::Code::OutOfRange => {
                return Err(status(code));
            }
            Some(Fault::Refuse(_)) => status(tonic::Code::OutOfRange),
            Some(Fault::LoseResponse) => {
                let response = self.inner.publish(request).await?;
                self.lost.lock().extend(response.envelope_metas);
                status(tonic::Code::OutOfRange)
            }
        };
        if self.stop_before_recovery.swap(false, Ordering::SeqCst) {
            self.poisoned.store(true, Ordering::SeqCst);
        }
        Err(out_of_range)
    }

    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }

    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        if self.poisoned.load(Ordering::SeqCst) {
            return Err(status(tonic::Code::DataLoss));
        }
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

    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
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
        self.inner.register(request).await
    }

    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.inner.unregister(request).await
    }

    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.update_subscriptions(request).await
    }
}

/// The tester's group in a new client over the same store, behind `api`.
/// A second call is a restart: only durable state carries over.
pub(super) async fn faulty_group(
    tester: &crate::utils::ClientTester,
    group_id: &GroupId,
    api: FaultyApi,
) -> MlsGroup<FaultyContext> {
    let mut builder = crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true));
    if let Some(provider) = tester.builder.config_provider.clone() {
        builder = builder.config_provider(provider);
    }
    let client = builder.build().await.unwrap();
    MlsGroup::new_cached(client.context.clone(), group_id)
        .unwrap()
        .0
}

/// The tester's group behind an API whose next group publishes fail with `statuses`.
pub(super) async fn refusing_group(
    tester: &crate::utils::ClientTester,
    group_id: &GroupId,
    statuses: impl IntoIterator<Item = tonic::Code>,
) -> MlsGroup<FaultyContext> {
    let api = FaultyApi::new(
        tester,
        group_message,
        statuses.into_iter().map(Fault::Refuse),
    );
    faulty_group(tester, group_id, api).await
}
