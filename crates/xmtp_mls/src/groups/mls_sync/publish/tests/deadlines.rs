use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use xmtp_common::time::{Duration, Instant, timeout};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

const BUDGET: Duration = Duration::from_millis(250);
const OUTER_BOUND: Duration = Duration::from_secs(2);

type StalledContext = Arc<
    crate::context::XmtpMlsLocalContext<
        StalledApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

#[derive(Clone, Copy)]
enum StalledCall {
    TargetQuery,
    WelcomePublish,
}

#[derive(Clone)]
struct StalledApi {
    inner: crate::utils::TestClient,
    call: StalledCall,
    entered: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl StalledApi {
    async fn never_reply<T>(&self) -> T {
        struct RequestGuard(Arc<AtomicBool>);
        impl Drop for RequestGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _request = RequestGuard(self.cancelled.clone());
        self.entered.store(true, Ordering::SeqCst);
        futures::future::pending().await
    }
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for StalledApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        match self.call {
            // Missing receipts leave the prepared attempt unresolved, so the
            // round must query its target before it can receive messages.
            StalledCall::TargetQuery => Ok(wire::PublishResponse::default()),
            StalledCall::WelcomePublish
                if request.envelopes.iter().any(|envelope| {
                    matches!(envelope.payload, Some(Payload::WelcomeMessage(_)))
                }) =>
            {
                self.never_reply().await
            }
            StalledCall::WelcomePublish => self.inner.publish(request).await,
        }
    }

    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.inner.query(request).await
    }

    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        match self.call {
            StalledCall::TargetQuery => self.never_reply().await,
            StalledCall::WelcomePublish => self.inner.query_newest(request).await,
        }
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
}

async fn client_with_stalled_api(
    tester: &crate::utils::ClientTester,
    call: StalledCall,
) -> crate::Client<StalledContext> {
    let api = StalledApi {
        inner: tester.context.api().api_client.clone(),
        call,
        entered: Arc::new(AtomicBool::new(false)),
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let mut settings = tester.context.incoming_runtime().policy().clone();
    settings.barrier_timeout = BUDGET;
    // Reuse registered tester state to inject one non-completing async RPC.
    crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .stream_policy(settings)
        .build()
        .await
        .unwrap()
}

#[xmtp_common::test(unwrap_try = true)]
async fn intent_sync_deadline_bounds_a_stalled_target_query() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"waiting for a target", Default::default())?;
    let (intent, _) = prepare_message(&group).await?;
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    let client = client_with_stalled_api(&alix, StalledCall::TargetQuery).await;
    let (group, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    let started = Instant::now();
    let result = timeout(OUTER_BOUND, group.sync_until_last_intent_resolved()).await?;
    assert!(matches!(result, Err(GroupError::SyncFailedToWait(_))));
    assert!(started.elapsed() < OUTER_BOUND);
    let api = &client.context.api().api_client;
    assert!(api.entered.load(Ordering::SeqCst));
    assert!(api.cancelled.load(Ordering::SeqCst));
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Published);
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        before
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn intent_sync_deadline_bounds_a_stalled_welcome_publish() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    group.publish_intents().await?;
    assert!(!group.receive().await?.is_errored());
    group.prepare_required_welcomes(intent.id)?.unwrap();
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    let client = client_with_stalled_api(&alix, StalledCall::WelcomePublish).await;
    let (group, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    let started = Instant::now();
    let result = timeout(OUTER_BOUND, group.sync_until_last_intent_resolved()).await?;
    assert!(matches!(
        result,
        Err(GroupError::PublishedButUnconfirmed { intent_id, .. }) if intent_id == intent.id
    ));
    assert!(started.elapsed() < OUTER_BOUND);
    let api = &client.context.api().api_client;
    assert!(api.entered.load(Ordering::SeqCst));
    assert!(api.cancelled.load(Ordering::SeqCst));
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Committed);
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        before
    );
}
