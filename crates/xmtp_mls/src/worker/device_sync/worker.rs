use super::{
    DeviceSyncClient, DeviceSyncError, ReceivedContent, decode_sync_message,
    preference_sync::{PreferenceUpdate, store_preference_updates},
};
use crate::{
    context::XmtpSharedContext,
    identity_updates::IdentityUpdates,
    subscriptions::{
        incoming::{IncomingCoordinator, IncomingScope},
        internal::{GroupOrigin, InternalEvent, PreferenceOrigin},
    },
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, NeedsDbReconnect, Worker, WorkerFactory,
        WorkerKind, WorkerResult, metrics::WorkerMetrics,
    },
};
use futures::TryFutureExt;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::OnceCell;
use tracing::instrument;
use xmtp_common::Event;
use xmtp_common::time::Instant;
use xmtp_cryptography::secret::Zeroizing;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_db::processed_device_sync_messages::SyncMessageKey;
use xmtp_events::{ClientEvent, Subscription};
use xmtp_macro::log_event;
use xmtp_proto::xmtp::device_sync::content::{
    DeviceSyncAcknowledge, PreferenceUpdate as PreferenceUpdateProto,
    PreferenceUpdates as PreferenceUpdatesProto, device_sync_content::Content as ContentProto,
};

pub(super) fn log_incoming_preference_updates(
    updates: &[xmtp_proto::xmtp::device_sync::content::PreferenceUpdate],
) {
    tracing::info!(update_count = updates.len(), "Incoming preference updates");
}

/// Records per sync message when a newcomer is sent the consent table
/// (about 100 KB, far below the 1 MiB envelope).
const CONSENT_REPLAY_PAGE: usize = 1_000;
/// A quarter of the 1 MiB envelope, so MLS and envelope framing always fit.
/// Honest pages (64-char ids) reach the 1,000-record bound first.
const CONSENT_REPLAY_PAGE_BYTES: usize = 256 * 1024;
const RETRY_FIRST: Duration = Duration::from_secs(1); // today's restart delay
const RETRY_MAX: Duration = Duration::from_secs(10 * 60);
/// 2 s doubling to 10 min: the 20th failure comes about two hours after the first.
const MAX_EVENT_FAILURES: u32 = 20;

fn backoff(n: u32) -> Duration {
    RETRY_FIRST.saturating_mul(1 << n.min(10)).min(RETRY_MAX)
}

/// Set when a failed or waiting event runs again: after its backoff, and never
/// before the delay of a rate limit that refused it, because the retry resends
/// the refused request.
// implements: RATE-041
fn schedule_retry(owed: &mut Owed, error: &DeviceSyncError) {
    let delay = xmtp_common::RetryableError::rate_limit_delay(error).unwrap_or_default();
    let wait = backoff(owed.failures + owed.waits).max(delay);
    owed.retry_at = xmtp_common::time::saturating_after(Instant::now(), wait);
}

/// Events not yet handled. Shared by the factory so a DB reconnect keeps them.
type PendingEvent = Arc<Mutex<Vec<Owed>>>;
/// When the own identity state may be read again for sync messages, after a
/// rate limit refused that read. Shared by the factory like [`PendingEvent`].
type OwnStateHold = Arc<Mutex<Option<Instant>>>;

#[derive(Clone)]
struct Owed {
    id: u64,
    event: xmtp_events::EventEnvelope<InternalEvent>,
    failures: u32,
    waits: u32,
    retry_at: Instant,
    cycled: bool,
}

impl Owed {
    fn new(id: u64, event: xmtp_events::EventEnvelope<InternalEvent>) -> Self {
        Self {
            id,
            event,
            failures: 0,
            waits: 0,
            retry_at: Instant::now(),
            cycled: false,
        }
    }

    fn kind(&self) -> &'static str {
        match (&self.event.client, &self.event.internal) {
            (Some(ClientEvent::IdentityOwnInstallationRevoked(_)), _) => "revocation",
            (_, Some(InternalEvent::GroupJoined { .. })) => "sync_welcome",
            (_, Some(InternalEvent::PreferencesChanged { .. })) => "preferences",
            _ => "sync_messages",
        }
    }
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, LazyLock, Weak},
    };
    use tokio::sync::Notify;
    use xmtp_db::consent_record::StoredConsentRecord;
    use xmtp_proto::types::InstallationId;

    use super::PreferenceUpdate;

    pub(crate) type ConsentKey = (i32, String);

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum PreferenceStage {
        Published,
        Received,
        Completed,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct ConsentObservation {
        pub(crate) installation: InstallationId,
        pub(crate) stage: PreferenceStage,
        pub(crate) record: StoredConsentRecord,
    }

    struct PreferenceCaptureState {
        keys: HashSet<ConsentKey>,
        records: parking_lot::Mutex<Vec<ConsentObservation>>,
        changed: Notify,
    }

    static PREFERENCE_CAPTURES: LazyLock<
        parking_lot::Mutex<HashMap<InstallationId, Weak<PreferenceCaptureState>>>,
    > = LazyLock::new(parking_lot::Mutex::default);

    pub(crate) struct PreferenceCapture {
        installations: Vec<InstallationId>,
        state: Arc<PreferenceCaptureState>,
    }

    impl PreferenceCapture {
        #[track_caller]
        pub(crate) fn new(
            installations: Vec<InstallationId>,
            records: &[StoredConsentRecord],
        ) -> Self {
            let state = Arc::new(PreferenceCaptureState {
                keys: records
                    .iter()
                    .map(|record| (record.entity_type as i32, record.entity.clone()))
                    .collect(),
                records: parking_lot::Mutex::default(),
                changed: Notify::new(),
            });
            let mut captures = PREFERENCE_CAPTURES.lock();
            for installation in &installations {
                assert!(
                    captures.get(installation).and_then(Weak::upgrade).is_none(),
                    "one preference capture per installation"
                );
                captures.insert(*installation, Arc::downgrade(&state));
            }
            Self {
                installations,
                state,
            }
        }

        pub(crate) fn snapshot(&self) -> Vec<ConsentObservation> {
            self.state.records.lock().clone()
        }

        pub(crate) async fn changed(&self) {
            self.state.changed.notified().await;
        }
    }

    impl Drop for PreferenceCapture {
        fn drop(&mut self) {
            let mut captures = PREFERENCE_CAPTURES.lock();
            for installation in &self.installations {
                if captures
                    .get(installation)
                    .is_some_and(|capture| Weak::ptr_eq(capture, &Arc::downgrade(&self.state)))
                {
                    captures.remove(installation);
                }
            }
        }
    }

    pub(crate) fn captures_preferences(installation: InstallationId) -> bool {
        PREFERENCE_CAPTURES
            .lock()
            .get(&installation)
            .and_then(Weak::upgrade)
            .is_some()
    }

    pub(crate) fn observe_preferences(
        installation: InstallationId,
        stage: PreferenceStage,
        updates: &[PreferenceUpdate],
    ) {
        let capture = PREFERENCE_CAPTURES
            .lock()
            .get(&installation)
            .and_then(Weak::upgrade);
        let Some(capture) = capture else {
            return;
        };
        let mut records = capture.records.lock();
        let before = records.len();
        for update in updates {
            if let PreferenceUpdate::Consent(record) = update
                && capture
                    .keys
                    .contains(&(record.entity_type as i32, record.entity.clone()))
            {
                records.push(ConsentObservation {
                    installation,
                    stage,
                    record: record.clone(),
                });
            }
        }
        let changed = records.len() != before;
        drop(records);
        if changed {
            capture.changed.notify_one();
        }
    }

    type BlockHook = (Vec<u8>, Arc<Notify>, Arc<Notify>);
    /// `(installation, observed, rate_limit)`: the next preference publish on
    /// that installation fails, with a rate limit of that delay when one is set.
    pub(crate) static FAIL_NEXT_PREFERENCE_PUBLISH: parking_lot::Mutex<Option<FailHook>> =
        parking_lot::Mutex::new(None);
    type FailHook = (Vec<u8>, Arc<Notify>, Option<xmtp_common::time::Duration>);
    pub(crate) static BLOCK_NEXT_PREFERENCE_PUBLISH: parking_lot::Mutex<Option<BlockHook>> =
        parking_lot::Mutex::new(None);
    /// `(installation, delay, reads)`: every own-state read of sync-message
    /// processing on that installation counts in `reads` and is refused with
    /// a rate limit of `delay`.
    pub(crate) static RATE_LIMIT_OWN_STATE: parking_lot::Mutex<Option<OwnStateRefusal>> =
        parking_lot::Mutex::new(None);
    pub(crate) type OwnStateRefusal = (
        Vec<u8>,
        xmtp_common::time::Duration,
        Arc<std::sync::atomic::AtomicUsize>,
    );

    /// `result`, unless [`RATE_LIMIT_OWN_STATE`] refuses this installation's read.
    pub(crate) fn refused_own_state<T>(
        installation: &[u8],
        result: Result<T, crate::client::ClientError>,
    ) -> Result<T, crate::client::ClientError> {
        let hook = RATE_LIMIT_OWN_STATE.lock().clone();
        match hook {
            Some((id, delay, reads)) if id == installation => {
                reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(crate::client::ClientError::Api(xmtp_api::ApiError::Api(
                    xmtp_proto::api::NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
                        xmtp_api_grpc::error::rate_limited_status("chain_verify", delay),
                    )),
                )))
            }
            _ => result,
        }
    }
}

#[cfg(test)]
struct TestTickGate {
    ready: futures::channel::oneshot::Sender<()>,
    release: futures::channel::oneshot::Receiver<()>,
}

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
    pending: PendingEvent,
    next_event_id: Arc<AtomicU64>,
    own_state_hold: OwnStateHold,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
    #[cfg(test)]
    completed_turns: Option<Arc<[AtomicU64; 2]>>,
    #[cfg(test)]
    received_ticks: Option<Arc<AtomicU64>>,
    #[cfg(test)]
    tick_gates: std::collections::VecDeque<TestTickGate>,
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn new(
        context: Context,
        metrics: Option<DynMetrics>,
        pending: PendingEvent,
        next_event_id: Arc<AtomicU64>,
        own_state_hold: OwnStateHold,
    ) -> Self {
        let metrics = metrics
            .and_then(|m| m.as_sync_metrics())
            .unwrap_or(Arc::new(WorkerMetrics::new(context.installation_id())));
        let client = DeviceSyncClient::new(context, metrics.clone());

        Self {
            client,
            subscription: None,
            pending,
            next_event_id,
            own_state_hold,
            init: OnceCell::new(),
            metrics,
            #[cfg(test)]
            completed_turns: None,
            #[cfg(test)]
            received_ticks: None,
            #[cfg(test)]
            tick_gates: Default::default(),
        }
    }
}

struct Factory<Context> {
    context: Context,
    pending: PendingEvent,
    next_event_id: Arc<AtomicU64>,
    own_state_hold: OwnStateHold,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        let worker = SyncWorker::new(
            self.context.clone(),
            metrics,
            self.pending.clone(),
            self.next_event_id.clone(),
            self.own_state_hold.clone(),
        );
        let metrics = worker.metrics.clone();

        (Box::new(worker) as Box<_>, Some(metrics as Arc<_>))
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }

    fn set_subscription(&mut self, subscription: Arc<Subscription<InternalEvent>>) {
        self.subscription = Some(subscription);
    }

    fn metrics(&self) -> Option<DynMetrics> {
        Some(self.metrics.clone())
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory {
            context,
            pending: Arc::default(),
            next_event_id: Arc::new(AtomicU64::new(1)),
            own_state_hold: Arc::default(),
        }
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        // Keep the large startup future off the worker poll stack.
        Box::pin(self.sync_init()).await?;
        // Receipt must outlive each sync call so remote updates can wake this worker.
        let _receipt = IncomingCoordinator::for_context(&self.client.context)
            .acquire(IncomingScope::DeviceSyncGroups);
        self.metrics.increment_metric(SyncMetric::Init);

        // Keep event futures off the containing worker poll stack.
        Box::pin(self.run_internal()).await
    }

    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        use futures::StreamExt;
        let (base, jitter) = self
            .client
            .context
            .worker_interval(WorkerKind::DeviceSync, Duration::from_secs(20));
        let intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        #[cfg(test)]
        let intervals = {
            let gates = Arc::new(Mutex::new(std::mem::take(&mut self.tick_gates)));
            let intervals = intervals.then(move |instant| {
                let gate = gates.lock().pop_front();
                async move {
                    if let Some(gate) = gate {
                        let _ = gate.ready.send(());
                        let _ = gate.release.await;
                    }
                    instant
                }
            });
            xmtp_common::wasm_or_native! {
                native => { intervals.boxed() },
                wasm => { intervals.boxed_local() },
            }
        };
        let mut intervals = intervals;
        // Poll events while native startup jitter delays the first tick.
        let mut skip_initial_tick = cfg!(not(target_arch = "wasm32"));
        let subscription = self
            .subscription
            .as_ref()
            .expect("runner installs subscription")
            .clone();
        loop {
            self.run_due().await?;
            let wait = self.next_retry_in();
            tokio::select! {
                event = subscription.next() => {
                    let Some(event) = event else { break; };
                    let id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
                    self.pending.lock().push(Owed::new(id, event));
                }
                _ = xmtp_common::time::sleep(wait.unwrap_or_default()), if wait.is_some() => {}
                _ = intervals.next() => {
                    #[cfg(test)]
                    if let Some(ticks) = &self.received_ticks {
                        ticks.fetch_add(1, Ordering::SeqCst);
                    }
                    if skip_initial_tick {
                        skip_initial_tick = false;
                        continue;
                    }
                    self.evt_new_sync_group_msg(true).await?;
                },
            }
        }
        Ok(())
    }

    /// Handles each event whose retry time has come. A failed event waits
    /// under backoff while later events run; one that fails
    /// `MAX_EVENT_FAILURES` times is dropped. Waiting for the own identity
    /// state or out a rate limit is not a failure.
    async fn run_due(&mut self) -> Result<(), DeviceSyncError> {
        let now = Instant::now();
        let mut due: Vec<_> = self
            .pending
            .lock()
            .iter()
            .filter(|owed| owed.retry_at <= now)
            .cloned()
            .collect();
        due.sort_by_key(|owed| owed.id);
        for mut owed in due {
            let result = Box::pin(self.handle_event(&mut owed)).await;
            let mut pending = self.pending.lock();
            pending.retain(|o| o.id != owed.id);
            match result {
                Ok(()) => {
                    #[cfg(test)]
                    if let Some(InternalEvent::PreferencesChanged {
                        updates,
                        origin: PreferenceOrigin::Local,
                    }) = &owed.event.internal
                    {
                        test_hooks::observe_preferences(
                            self.client.context.installation_id(),
                            test_hooks::PreferenceStage::Completed,
                            updates,
                        );
                    }
                }
                Err(e) if e.needs_db_reconnect() => {
                    pending.push(owed);
                    return Err(e);
                }
                // implements: SYNC-024. Waiting for the own state is not a failure.
                Err(e @ DeviceSyncError::OwnStateUnavailable(_)) => {
                    owed.waits += 1;
                    schedule_retry(&mut owed, &e);
                    tracing::info!(error = %e, "device sync event waits for the own identity state");
                    pending.push(owed);
                }
                // A rate limit is not a failure either: the event runs again
                // after the delay and is never dropped for it.
                Err(e) if xmtp_common::RetryableError::rate_limit_delay(&e).is_some() => {
                    owed.waits += 1;
                    schedule_retry(&mut owed, &e);
                    tracing::info!(error = %e, "device sync event waits out a rate limit");
                    pending.push(owed);
                }
                Err(e) => {
                    owed.failures += 1;
                    if owed.failures >= MAX_EVENT_FAILURES {
                        tracing::warn!(error = %e, kind = owed.kind(), "dropping a device sync event after 20 failures");
                        self.metrics.increment_metric(SyncMetric::EventDropped);
                        continue;
                    }
                    schedule_retry(&mut owed, &e);
                    tracing::warn!(error = %e, failures = owed.failures, "device sync event failed");
                    pending.push(owed);
                }
            }
        }
        Ok(())
    }

    fn next_retry_in(&self) -> Option<Duration> {
        let next = self.pending.lock().iter().map(|owed| owed.retry_at).min()?;
        Some(next.saturating_duration_since(Instant::now()))
    }

    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn"))]
    async fn handle_event(&mut self, owed: &mut Owed) -> Result<(), DeviceSyncError> {
        if matches!(
            owed.event.client,
            Some(ClientEvent::IdentityOwnInstallationRevoked(_))
        ) {
            self.evt_cycle_hmac().await?;
        }
        match owed.event.internal.clone() {
            Some(InternalEvent::GroupJoined {
                is_sync: true,
                origin: GroupOrigin::Welcomed,
                ..
            }) => self.evt_new_sync_group_from_welcome(owed).await,
            Some(
                InternalEvent::MessageStored { is_sync: true, .. }
                | InternalEvent::SyncMessagePublished,
            ) => self.evt_new_sync_group_msg(false).await,
            Some(InternalEvent::PreferencesChanged {
                updates,
                origin: PreferenceOrigin::Local,
            }) => self.evt_sync_preferences(updates).await,
            _ => Ok(()),
        }
    }

    /// Initialize the sync group when the client is registered.
    #[instrument(level = "trace", skip_all)]
    async fn sync_init(&mut self) -> Result<(), DeviceSyncError> {
        let Self { init, client, .. } = &self;

        init.get_or_try_init(|| async {
            log_event!(
                Event::DeviceSyncInitializing,
                self.client.context.installation_id()
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if client.primary_sync_group()?.is_none() {
                log_event!(
                    Event::DeviceSyncNoPrimarySyncGroup,
                    self.client.context.installation_id()
                );
                // Sync-group creation polls membership publication below this call.
                let sync_group = Box::pin(client.get_sync_group()).await?;
                log_event!(
                    Event::DeviceSyncCreatedPrimarySyncGroup,
                    self.client.context.installation_id(),
                    group_id = sync_group.group_id
                );
            }

            log_event!(
                Event::DeviceSyncInitializingFinished,
                self.client.context.installation_id()
            );

            Ok(())
        })
        .await
        .copied()
    }

    async fn evt_new_sync_group_from_welcome(
        &self,
        owed: &mut Owed,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("New sync group from welcome detected.");

        // A new sync group from a welcome indicates a new installation.
        // Schedule durable per-group reconciliation on the TaskRunner —
        // a one-shot inline add here is lost forever if it fails once.
        self.client.schedule_add_installations_to_groups()?;

        self.metrics
            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

        // implements: SYNC-015. One root key per Welcome, however often the replay is retried.
        if !owed.cycled {
            self.client.cycle_hmac().await?;
            owed.cycled = true;
        }

        // implements: SYNC-025, SYNC-021
        // Every record with a time travels at its stored time; a default at
        // time 0 stays local.
        let records: Vec<_> = self
            .client
            .db()
            .consent_records()?
            .into_iter()
            .filter(|r| r.consented_at_ns > 0)
            .map(PreferenceUpdate::Consent)
            .collect();
        // implements: SYNC-025. One page that fails does not stop the others.
        let mut first_error = None;
        for page in replay_pages(records) {
            if let Err(e) = self.client.sync_preferences(page).await {
                if e.needs_db_reconnect() {
                    return Err(e);
                }
                first_error.get_or_insert(e);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Process the stored sync messages. When a rate limit refuses the own
    /// identity state they need, the messages stay pending and no turn, at a
    /// tick or for a new message, reads that state again inside the delay.
    // implements: RATE-041
    async fn evt_new_sync_group_msg(&self, is_tick: bool) -> Result<(), DeviceSyncError> {
        if self
            .own_state_hold
            .lock()
            .is_some_and(|until| Instant::now() < until)
        {
            return Ok(());
        }
        tracing::debug!(is_tick, "processing sync group messages");
        let mut result = self.client.process_sync_group_messages(&self.metrics).await;
        if let Err(DeviceSyncError::OwnStateUnavailable(error)) = &result
            && let Some(delay) = xmtp_common::RetryableError::rate_limit_delay(error)
        {
            tracing::warn!(%error, "own identity state rate limited; sync messages stay pending");
            *self.own_state_hold.lock() =
                Some(xmtp_common::time::saturating_after(Instant::now(), delay));
            result = Ok(());
        }
        #[cfg(test)]
        if result.is_ok()
            && let Some(turns) = &self.completed_turns
        {
            turns[usize::from(is_tick)].fetch_add(1, Ordering::SeqCst);
        }
        result
    }

    async fn evt_sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<(), DeviceSyncError> {
        #[cfg(test)]
        {
            let blocked = {
                let mut hook = test_hooks::BLOCK_NEXT_PREFERENCE_PUBLISH.lock();
                if hook.as_ref().is_some_and(|(installation, _, _)| {
                    installation.as_slice()
                        == self.client.context.installation_id().to_vec().as_slice()
                }) {
                    hook.take()
                } else {
                    None
                }
            };
            if let Some((_, entered, release)) = blocked {
                entered.notify_one();
                release.notified().await;
            }
            let failed = {
                let mut hook = test_hooks::FAIL_NEXT_PREFERENCE_PUBLISH.lock();
                if hook.as_ref().is_some_and(|(installation, _, _)| {
                    installation.as_slice()
                        == self.client.context.installation_id().to_vec().as_slice()
                }) {
                    hook.take()
                } else {
                    None
                }
            };
            if let Some((_, observed, rate_limit)) = failed {
                observed.notify_one();
                if let Some(delay) = rate_limit {
                    return Err(DeviceSyncError::Group(
                        crate::groups::GroupError::WrappedApi(xmtp_api::ApiError::Api(
                            xmtp_proto::api::NetworkError::new(
                                xmtp_api_grpc::error::GrpcError::Status(
                                    xmtp_api_grpc::error::rate_limited_status("publish", delay),
                                ),
                            ),
                        )),
                    ));
                }
                return Err(DeviceSyncError::IO(std::io::Error::other(
                    "test preference publication failure",
                )));
            }
        }
        self.client.sync_preferences(updates).await?;
        Ok(())
    }

    async fn evt_cycle_hmac(&self) -> Result<(), DeviceSyncError> {
        self.client.cycle_hmac().await?;
        Ok(())
    }
}

/// Pages of at most 1,000 records and 256 KiB of encoded updates. A record
/// larger than the byte bound goes in a page of its own.
fn replay_pages(updates: Vec<PreferenceUpdate>) -> Vec<Vec<PreferenceUpdate>> {
    let (mut pages, mut bytes): (Vec<Vec<_>>, usize) = (vec![], 0);
    for update in updates {
        let len = prost::Message::encoded_len(&PreferenceUpdateProto::from(update.clone()));
        match pages.last_mut() {
            Some(page)
                if page.len() < CONSENT_REPLAY_PAGE && bytes + len <= CONSENT_REPLAY_PAGE_BYTES =>
            {
                bytes += len;
                page.push(update);
            }
            _ => {
                bytes = len;
                pages.push(vec![update]);
            }
        }
    }
    pages
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    pub(super) async fn process_sync_group_messages(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.installation_id();
        let (mut after, mut own) = (None, None);
        loop {
            let page = self
                .context
                .db()
                .unprocessed_sync_group_messages(after.as_ref())?;
            let Some(last) = page.last() else {
                return Ok(());
            };
            after = Some(SyncMessageKey::of(last));
            tracing::info!("Processing {} messages.", page.len());
            let own_installations = match &own {
                Some(own) => own,
                None => {
                    // implements: SYNC-016 (a failed own-state read, such as a smart-wallet
                    // signature that needs the verifier, leaves every row Pending for the next tick)
                    let own_state = IdentityUpdates::new(&self.context)
                        .get_association_state(&self.context.db(), self.context.inbox_id(), None)
                        .await;
                    #[cfg(test)]
                    let own_state = test_hooks::refused_own_state(
                        &self.context.installation_id().to_vec(),
                        own_state,
                    );
                    match own_state.map_err(DeviceSyncError::from) {
                        Ok(state) => own.insert(state.installation_ids()),
                        Err(err) if err.needs_db_reconnect() => return Err(err),
                        // A rate-limited read is returned, so the worker holds it for the
                        // delay (RATE-041).
                        Err(DeviceSyncError::Client(err))
                            if xmtp_common::RetryableError::rate_limit_delay(&err).is_some() =>
                        {
                            return Err(DeviceSyncError::OwnStateUnavailable(err));
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "own identity state unavailable; sync messages stay pending");
                            return Ok(());
                        }
                    }
                }
            };

            for mut msg in page {
                // The plaintext and its inner encoding can hold an HMAC root key.
                // Both are wiped on every path, including skipped messages.
                let plaintext = Zeroizing::new(std::mem::take(&mut msg.decrypted_message_bytes));
                // Older builds stored sync groups from other inboxes.
                // Only current installations of the own inbox may send updates.
                // implements: SYNC-011
                if msg.sender_inbox_id != self.context.inbox_id()
                    || !own_installations.contains(&msg.sender_installation_id)
                {
                    tracing::warn!(
                        group_id = %msg.group_id,
                        sender_inbox_id = %msg.sender_inbox_id,
                        "dropping a sync message from a sender that is not an installation of the own inbox"
                    );
                    self.context
                        .db()
                        .mark_device_sync_msg_as_processed(&msg.id)?;
                    continue;
                }
                let content = decode_sync_message(plaintext.as_slice());
                let Some(content) = content else {
                    // Older installations can send archive transfer content. Its
                    // reserved oneof fields decode as unsupported content here.
                    // Mark it complete so the worker does not retry it forever.
                    self.context
                        .db()
                        .mark_device_sync_msg_as_processed(&msg.id)?;
                    continue;
                };
                let is_external = msg.sender_installation_id != installation_id;

                let msg_type = match &content.0 {
                    ContentProto::PreferenceUpdates(_) => "PreferenceUpdates",
                    ContentProto::Acknowledge(_) => "Acknowledge",
                };

                log_event!(
                    Event::DeviceSyncProcessingMessages,
                    self.context.installation_id(),
                    msg_type,
                    external = is_external,
                    message_id = #msg.id,
                    group_id = msg.group_id
                );

                if let Err(err) = self.process_message(handle, &msg, content).await {
                    // implements: SYNC-016 (a storage error leaves the row Pending for the next tick)
                    // A failed message is non-fatal, but a dropped pool must stop the worker.
                    if err.needs_db_reconnect() {
                        return Err(err);
                    }
                    log_event!(
                        Event::DeviceSyncMessageProcessingError,
                        self.context.installation_id(),
                        error = %err,
                        message_id = #msg.id
                    );
                } else {
                    self.context
                        .db()
                        .mark_device_sync_msg_as_processed(&msg.id)?;
                }
            }
        }
    }

    async fn process_message(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        msg: &StoredGroupMessage,
        content: ReceivedContent,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.context.installation_id();
        let is_external = msg.sender_installation_id != installation_id;

        match &content.0 {
            ContentProto::PreferenceUpdates(PreferenceUpdatesProto { updates }) => {
                #[cfg(test)]
                let observed_updates =
                    test_hooks::captures_preferences(installation_id).then(|| updates.clone());
                if is_external {
                    log_incoming_preference_updates(updates);
                }
                // implements: PROC-036
                tracing::info!(update_count = updates.len(), "storing preference updates");
                // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                crate::state_tx::state_write_with_events(
                    self.context.mls_storage(),
                    self.context.events(),
                    |tx, events| {
                        let storage = tx.storage();
                        let db = storage.db();
                        let updated = store_preference_updates(updates, &db, handle)?;
                        crate::subscriptions::internal::emit_preference_updates_with_public(
                            events,
                            updated.public,
                            updated.legacy.clone(),
                            PreferenceOrigin::Sync,
                            &db,
                        )?;
                        if !updated.legacy.is_empty() {
                            self.context.task_channels().mark_notification_changed();
                        }
                        Ok::<_, xmtp_db::StorageError>(xmtp_db::TransactionOutcome::Continue(
                            updated.legacy,
                        ))
                    },
                )?
                .into_continued();
                #[cfg(test)]
                if let Some(observed_updates) = observed_updates {
                    test_hooks::observe_preferences(
                        installation_id,
                        test_hooks::PreferenceStage::Received,
                        &observed_updates
                            .into_iter()
                            .filter_map(|update| PreferenceUpdate::try_from(update).ok())
                            .collect::<Vec<_>>(),
                    );
                }
            }
            ContentProto::Acknowledge(DeviceSyncAcknowledge { .. }) => {
                return Ok(());
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    SyncGroupWelcomesProcessed,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
    EventDropped,
}

impl WorkerMetrics<SyncMetric> {
    pub async fn wait_for_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.register_interest(SyncMetric::Init, 1).wait().await
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::{tester, worker::WorkerConfig};
    use futures::FutureExt;
    use xmtp_events::EventWriter;

    #[cfg(not(target_arch = "wasm32"))]
    fn welcome() -> Owed {
        let joined = InternalEvent::GroupJoined {
            group_id: Default::default(),
            is_sync: true,
            origin: GroupOrigin::Welcomed,
        };
        Owed::new(
            0,
            xmtp_events::EventEnvelope::new(None, Some(joined), Default::default()),
        )
    }

    fn idle_worker<C: XmtpSharedContext + 'static>(context: C) -> SyncWorker<C> {
        SyncWorker::new(
            context,
            None,
            Arc::default(),
            Arc::new(AtomicU64::new(0)),
            Arc::default(),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn consent_published_before_primary_adoption_reaches_new_installation() {
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

        tester!(source, disable_workers);
        let source_sync = source.device_sync_client();
        let old_primary = source_sync.get_sync_group().await?;
        let group = source.create_group(None, None)?;
        let record = StoredConsentRecord {
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Denied,
            entity: hex::encode(group.group_id),
            consented_at_ns: xmtp_common::time::now_ns(),
        };
        source
            .set_consent_states(std::slice::from_ref(&record))
            .await?;
        let record = source
            .context
            .db()
            .get_consent_record(record.entity.clone(), record.entity_type)?
            .unwrap();

        tester!(copy, from: source, disable_workers);
        let copy_sync = copy.device_sync_client();
        let new_primary = copy_sync.get_sync_group().await?;
        assert_ne!(old_primary.group_id, new_primary.group_id);
        assert_eq!(
            source_sync.primary_sync_group()?.unwrap().group_id,
            old_primary.group_id
        );
        assert!(
            copy.context
                .db()
                .find_group(&old_primary.group_id)?
                .is_none()
        );

        // Publish before the source can adopt the new installation's group.
        source_sync
            .sync_preferences(vec![PreferenceUpdate::Consent(record.clone())])
            .await?;
        source.sync_welcomes().await?;
        assert_eq!(
            source_sync.primary_sync_group()?.unwrap().group_id,
            new_primary.group_id
        );
        assert!(
            copy.context
                .db()
                .get_consent_record(record.entity.clone(), record.entity_type)?
                .is_none()
        );

        // Run the actual welcome handler, then consume its primary-group messages.
        let source_worker = idle_worker(source.context.clone());
        source_worker
            .evt_new_sync_group_from_welcome(&mut welcome())
            .await?;
        new_primary.sync().await?;
        let copy_worker = idle_worker(copy.context.clone());
        copy_worker.evt_new_sync_group_msg(false).await?;
        let received = copy
            .context
            .db()
            .get_consent_record(record.entity.clone(), record.entity_type)?;
        assert_eq!(
            received.map(|r| (r.state, r.consented_at_ns)),
            Some((record.state, record.consented_at_ns)),
            "a consent published before primary adoption must reach the new installation"
        );
        source.close().await?;
        copy.close().await?;
    }

    // verifies: SYNC-021, SYNC-025
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn join_sends_every_timed_record_at_its_time() {
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
        use xmtp_proto::xmtp::device_sync::content::preference_update::Update;
        let record = |entity: &str, state, consented_at_ns| StoredConsentRecord {
            entity_type: ConsentType::InboxId,
            state,
            entity: entity.into(),
            consented_at_ns,
        };

        tester!(alix1, disable_workers);
        tester!(alix2, from: alix1, disable_workers);
        // b alone exceeds the envelope; c, d and e each fill a byte-bounded page.
        // Inserted in name order, so rowid and key scans agree.
        let sized: Vec<_> = [("a-small", 1), ("b-huge", 1_100_000)]
            .into_iter()
            .chain(["c-big", "d-big", "e-big"].map(|name| (name, 400_000)))
            .chain([("f-small", 1)])
            .map(|(name, len)| {
                record(
                    &format!("{name}{}", "-".repeat(len)),
                    ConsentState::Allowed,
                    4,
                )
            })
            .collect();
        // With f and x, y, the fillers fill a 1,000-record page and start another.
        let fillers: Vec<_> = (0..999)
            .map(|i| record(&format!("filler-{i}"), ConsentState::Allowed, 5))
            .collect();
        // Stored as an archive import stores them, not as decisions.
        for stored in sized.iter().chain(&fillers).cloned().chain([
            record("x", ConsentState::Unknown, 1),
            record("y", ConsentState::Allowed, 3),
            record("z", ConsentState::Allowed, 0),
        ]) {
            alix1.context.db().insert_newer_consent_record(stored)?;
        }
        for held in [
            record("x", ConsentState::Denied, 2),
            record("y", ConsentState::Denied, 2),
        ] {
            alix2.context.db().insert_newer_consent_record(held)?;
        }
        let new_primary = alix2.device_sync_client().get_sync_group().await?;
        alix1.sync_welcomes().await?;
        let alix1_primary = alix1.device_sync_client().primary_sync_group()?;
        assert_eq!(alix1_primary.unwrap().group_id, new_primary.group_id);

        let replay = idle_worker(alix1.context.clone())
            .evt_new_sync_group_from_welcome(&mut welcome())
            .await;
        assert!(replay.is_err(), "the page that holds b fails");
        new_primary.sync().await?;
        let consent_messages = alix2
            .context
            .db()
            .unprocessed_sync_group_messages(None)?
            .iter()
            .filter_map(|msg| decode_sync_message(&msg.decrypted_message_bytes))
            .filter(|content| {
                matches!(&content.0, ContentProto::PreferenceUpdates(p)
                    if p.updates.iter().any(|u| matches!(u.update, Some(Update::Consent(_)))))
            })
            .count();
        assert_eq!(
            consent_messages, 6,
            "a, c, d, e, f with 999 fillers, then x and y; b fails alone"
        );
        idle_worker(alix2.context.clone())
            .evt_new_sync_group_msg(false)
            .await?;

        let db = alix2.context.db();
        let held = |entity: &str| {
            let held = db.get_consent_record(entity.into(), ConsentType::InboxId)?;
            Ok::<_, xmtp_db::ConnectionError>(held.map(|r| (r.state, r.consented_at_ns)))
        };
        assert_eq!(
            held("x")?,
            Some((ConsentState::Denied, 2)),
            "an older forwarded record must not replace a newer one"
        );
        assert_eq!(held("y")?, Some((ConsentState::Allowed, 3)));
        assert_eq!(held("z")?, None, "a default at time 0 must stay local");
        for filler in &fillers {
            assert_eq!(
                held(&filler.entity)?,
                Some((filler.state, filler.consented_at_ns))
            );
        }
        for (i, sized) in sized.iter().enumerate() {
            let expected = (i != 1).then_some((sized.state, sized.consented_at_ns));
            assert_eq!(held(&sized.entity)?, expected, "{}", &sized.entity[..6]);
        }
        alix1.close().await?;
        alix2.close().await?;
    }

    // verifies: SYNC-015
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_retry_cycles_the_root_key_once() {
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
        tester!(alix, configured: |c| c.limits.max_envelope_bytes = 65_536, disable_workers);
        alix.device_sync_client().get_sync_group().await?;
        // 1,000 records of 64 characters exceed the lowered envelope.
        let records: Vec<_> = (0..1_001)
            .map(|i| StoredConsentRecord {
                entity_type: ConsentType::InboxId,
                state: ConsentState::Allowed,
                entity: format!("{i:064}"),
                consented_at_ns: 1,
            })
            .collect();
        for chunk in records.chunks(200) {
            alix.context.db().record_app_act(chunk)?;
        }
        let worker = idle_worker(alix.context.clone());
        let mut owed = welcome();
        for _ in 0..2 {
            let replay = worker.evt_new_sync_group_from_welcome(&mut owed).await;
            assert!(replay.is_err(), "the full page exceeds the envelope");
        }
        assert_eq!(worker.metrics.get(SyncMetric::HmacSent), 1);
        alix.close().await?;
    }

    // verifies: SYNC-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn event_is_dropped_after_twenty_failures() {
        use super::super::tests::hide_own_identity_updates;
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
        tester!(alix, disable_workers);
        let record =
            StoredConsentRecord::new(ConsentType::InboxId, ConsentState::Allowed, "drop".into());
        // `wait` is 0 for a failure, 1 for an unreadable own state, and 2 for
        // a rate limit; only the 20th failure drops the event.
        for (failures, wait) in [(18, 0), (19, 0), (19, 1), (19, 2)] {
            let dropped = failures == 19 && wait == 0;
            let mut worker = idle_worker(alix.context.clone());
            let changed = InternalEvent::PreferencesChanged {
                updates: vec![PreferenceUpdate::Consent(record.clone())],
                origin: PreferenceOrigin::Local,
            };
            let event = xmtp_events::EventEnvelope::new(None, Some(changed), Default::default());
            let now = Instant::now();
            worker.pending.lock().push(Owed {
                failures,
                retry_at: now,
                ..Owed::new(1, event)
            });
            let restore = if wait == 1 {
                worker.client.get_sync_group().await?;
                hide_own_identity_updates(alix.context.db(), alix.inbox_id()).map(Some)?
            } else {
                let rate_limit = (wait == 2).then_some(Duration::from_millis(1));
                *test_hooks::FAIL_NEXT_PREFERENCE_PUBLISH.lock() = Some((
                    alix.context.installation_id().to_vec(),
                    Default::default(),
                    rate_limit,
                ));
                None
            };
            worker.run_due().await?;
            let pending = worker.pending.lock().clone();
            let dropped_count = worker.metrics.get(SyncMetric::EventDropped);
            if dropped {
                assert!(pending.is_empty(), "the 20th failure drops the event");
                assert_eq!(dropped_count, 1);
            } else {
                assert_eq!(pending.len(), 1, "the 19th failure keeps the event");
                assert_eq!(pending[0].failures, 19);
                assert_eq!(pending[0].waits, u32::from(wait > 0));
                assert!(pending[0].retry_at >= now + RETRY_MAX);
                assert_eq!(dropped_count, 0);
            }
            assert_eq!(worker.metrics.get(SyncMetric::ConsentSent), 0);
            if let Some(restore) = restore {
                restore()?;
                worker.pending.lock()[0].retry_at = Instant::now();
                worker.run_due().await?;
                assert!(worker.pending.lock().is_empty(), "sent once readable");
                assert_eq!(worker.metrics.get(SyncMetric::ConsentSent), 1);
                assert_eq!(worker.metrics.get(SyncMetric::EventDropped), 0);
            }
        }
        alix.close().await?;
    }

    /// A sync-message turn whose own-state read is refused by a rate limit
    /// leaves the messages pending, and no turn reads that state again inside
    /// the delay: not at a tick, not for a new message, and not in a worker
    /// rebuilt after a storage reconnect. After the delay the messages apply.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_own_state_read_waits_out_its_delay() {
        use std::sync::atomic::AtomicUsize;
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
        tester!(alix, disable_workers);
        let record =
            StoredConsentRecord::new(ConsentType::InboxId, ConsentState::Denied, "held".into());
        alix.device_sync_client()
            .sync_preferences(vec![PreferenceUpdate::Consent(record.clone())])
            .await?;
        let db = alix.context.db();
        assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);
        let reads = Arc::new(AtomicUsize::new(0));
        *test_hooks::RATE_LIMIT_OWN_STATE.lock() = Some((
            alix.context.installation_id().to_vec(),
            Duration::from_secs(300),
            reads.clone(),
        ));
        let hold = OwnStateHold::default();
        let worker = || {
            SyncWorker::new(
                alix.context.clone(),
                None,
                Arc::default(),
                Arc::new(AtomicU64::new(0)),
                hold.clone(),
            )
        };
        let first = worker();
        first.evt_new_sync_group_msg(true).await?;
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        first.evt_new_sync_group_msg(true).await?;
        first.evt_new_sync_group_msg(false).await?;
        worker().evt_new_sync_group_msg(true).await?;
        assert_eq!(reads.load(Ordering::SeqCst), 1, "no read inside the delay");
        assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);

        *test_hooks::RATE_LIMIT_OWN_STATE.lock() = None;
        *hold.lock() = Some(Instant::now());
        first.evt_new_sync_group_msg(true).await?;
        assert!(db.unprocessed_sync_group_messages(None)?.is_empty());
        assert_eq!(
            db.get_consent_record(record.entity.clone(), record.entity_type)?
                .map(|r| (r.state, r.consented_at_ns)),
            Some((record.state, record.consented_at_ns))
        );
    }

    /// A sync event refused by a rate limit runs again only after the delay,
    /// even while its backoff is shorter, whether it failed or waits for the
    /// own identity state. A delay too large for the clock does not panic.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    fn a_rate_limited_event_retries_after_the_delay() {
        let refused = |status| {
            xmtp_api::ApiError::Api(xmtp_proto::api::NetworkError::new(
                xmtp_api_grpc::error::GrpcError::Status(status),
            ))
        };
        let limited = |delay| xmtp_api_grpc::error::rate_limited_status("request", delay);
        let failed =
            |status| DeviceSyncError::Group(crate::groups::GroupError::WrappedApi(refused(status)));
        let waiting = |status| {
            DeviceSyncError::OwnStateUnavailable(crate::client::ClientError::Api(refused(status)))
        };
        let owed = || {
            let event = xmtp_events::EventEnvelope::new(None, None, Default::default());
            Owed {
                failures: 1,
                waits: 1,
                ..Owed::new(0, event)
            }
        };
        let delay = Duration::from_secs(300);
        assert!(backoff(2) < delay);
        for error in [failed(limited(delay)), waiting(limited(delay))] {
            let mut owed = owed();
            let before = Instant::now();
            schedule_retry(&mut owed, &error);
            assert!(owed.retry_at >= before + delay, "{error}");
        }
        let huge = xmtp_api_grpc::error::rate_limited_status_raw(i64::MAX, 0);
        let mut owed = owed();
        schedule_retry(&mut owed, &failed(huge));
        assert!(owed.retry_at > Instant::now() + RETRY_MAX);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn worker_turn_span_omits_hmac_key() {
        tester!(alix, disable_workers);
        let mut worker = idle_worker(alix.context.clone());
        let key = (1_u8..=42).collect::<Vec<_>>();
        let key_debug = format!("{key:?}");
        let event = xmtp_events::EventEnvelope::new(
            None,
            Some(InternalEvent::PreferencesChanged {
                updates: vec![PreferenceUpdate::Hmac {
                    key: key.into(),
                    cycled_at_ns: 1,
                }],
                origin: PreferenceOrigin::Sync,
            }),
            Default::default(),
        );
        let (result, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            worker
                .handle_event(&mut Owed::new(0, event))
                .now_or_never()
                .expect("the Sync-origin handler must finish without IO")
        });
        result?;
        let span = spans
            .iter()
            .find(|span| span.name == "handle_event")
            .expect("the actual worker handler must export its span");
        let fields = format!("{:?}", span.attributes);
        assert!(fields.contains("worker_turn"));
        assert!(fields.contains("DeviceSync"));
        assert!(
            !fields.contains(&key_debug),
            "worker span must not export the HMAC root key"
        );
        alix.close().await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn queued_event_precedes_first_periodic_turn() {
        const PERIOD: Duration = Duration::from_secs(4);
        const EVENT_WAIT: Duration = Duration::from_secs(1);
        const PERIOD_WAIT: Duration = Duration::from_secs(6);
        let config = WorkerConfig {
            interval_overrides: [(WorkerKind::DeviceSync, PERIOD.as_nanos() as u64)]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        tester!(alix, disable_workers, worker_config: config);
        let turns = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let mut worker = idle_worker(alix.context.clone());
        worker.completed_turns = Some(turns.clone());
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        alix.context
            .events()
            .emit(None, Some(InternalEvent::SyncMessagePublished));

        {
            let run = worker.run_internal().fuse();
            let observe = async {
                xmtp_common::time::timeout(
                    EVENT_WAIT,
                    xmtp_common::wait_for_eq(|| async { turns[0].load(Ordering::SeqCst) }, 1),
                )
                .await
                .expect("queued event must complete before the first periodic tick")
                .expect("queued event wait must complete");
                assert_eq!(turns[1].load(Ordering::SeqCst), 0);
                xmtp_common::time::timeout(
                    PERIOD_WAIT,
                    xmtp_common::wait_for_eq(
                        || async { turns[1].load(Ordering::SeqCst) >= 1 },
                        true,
                    ),
                )
                .await
                .expect("periodic reconciliation must still complete")
                .expect("periodic turn wait must complete");
                assert_eq!(turns[0].load(Ordering::SeqCst), 1);
            }
            .fuse();
            futures::pin_mut!(run, observe);
            futures::select! {
                () = observe => {},
                result = run => panic!("worker ended before observed turns: {result:?}"),
            }
        }
        subscription.close();
        alix.close().await?;
    }
    struct TickControl {
        gate: TestTickGate,
        ready: futures::channel::oneshot::Receiver<()>,
        release: futures::channel::oneshot::Sender<()>,
    }

    impl TickControl {
        fn new() -> Self {
            let (ready_tx, ready) = futures::channel::oneshot::channel();
            let (release, release_rx) = futures::channel::oneshot::channel();
            Self {
                gate: TestTickGate {
                    ready: ready_tx,
                    release: release_rx,
                },
                ready,
                release,
            }
        }
    }

    fn jitter_config() -> WorkerConfig {
        const PERIOD: Duration = Duration::from_millis(40);
        const JITTER: Duration = Duration::from_millis(20);
        WorkerConfig {
            interval_overrides: [(WorkerKind::DeviceSync, PERIOD.as_nanos() as u64)]
                .into_iter()
                .collect(),
            jitter_overrides: [(WorkerKind::DeviceSync, JITTER.as_nanos() as u64)]
                .into_iter()
                .collect(),
            ..Default::default()
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn jittered_initial_tick_does_not_block_events_or_change_first_turn() {
        const WAIT: Duration = Duration::from_secs(1);
        tester!(alix, disable_workers, worker_config: jitter_config());
        let turns = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let ticks = Arc::new(AtomicU64::new(0));
        let mut worker = idle_worker(alix.context.clone());
        worker.completed_turns = Some(turns.clone());
        worker.received_ticks = Some(ticks.clone());
        let first = TickControl::new();
        let second = TickControl::new();
        worker.tick_gates.extend([first.gate, second.gate]);
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        {
            let run = worker.run_internal().fuse();
            let observe = async {
                xmtp_common::time::timeout(WAIT, first.ready)
                    .await
                    .expect("actual jittered first tick must reach the private gate")
                    .expect("first tick gate must remain owned");
                alix.context
                    .events()
                    .emit(None, Some(InternalEvent::SyncMessagePublished));
                xmtp_common::time::timeout(
                    WAIT,
                    xmtp_common::wait_for_eq(|| async { turns[0].load(Ordering::SeqCst) }, 1),
                )
                .await
                .expect("queued event must complete while the initial tick is pending")
                .expect("queued event wait must complete");
                assert_eq!(turns[1].load(Ordering::SeqCst), 0);
                first.release.send(()).expect("release the first tick");
                xmtp_common::time::timeout(WAIT, second.ready)
                    .await
                    .expect("actual jittered second tick must reach the private gate")
                    .expect("second tick gate must remain owned");
                assert_eq!(ticks.load(Ordering::SeqCst), 1);
                let first_periodic = u64::from(cfg!(target_arch = "wasm32"));
                assert_eq!(
                    turns[1].load(Ordering::SeqCst),
                    first_periodic,
                    "native skips its first tick; Wasm reconciles its first tick"
                );
                second.release.send(()).expect("release the second tick");
                xmtp_common::time::timeout(
                    WAIT,
                    xmtp_common::wait_for_eq(
                        || async { turns[1].load(Ordering::SeqCst) > first_periodic },
                        true,
                    ),
                )
                .await
                .expect("periodic reconciliation must complete after the second tick")
                .expect("periodic reconciliation wait must complete");
            }
            .fuse();
            futures::pin_mut!(run, observe);
            futures::select! {
                () = observe => {},
                result = run => panic!("worker ended before observed turns: {result:?}"),
            }
        }
        subscription.close();
        alix.close().await?;
    }

    // verifies: SYNC-021
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_event_does_not_block_later_events() {
        use test_hooks::{PreferenceCapture, PreferenceStage};
        use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
        const WAIT: Duration = Duration::from_secs(10);
        tester!(alix, disable_workers, worker_config: jitter_config());
        alix.device_sync_client().get_sync_group().await?;
        let record = |entity: &str| {
            StoredConsentRecord::new(ConsentType::InboxId, ConsentState::Allowed, entity.into())
        };
        let (a, b) = (record("failing-a"), record("later-b"));
        let installation = alix.context.installation_id();
        let capture = PreferenceCapture::new(vec![installation], &[a.clone(), b.clone()]);
        let completed = |entity: &str| {
            capture
                .snapshot()
                .iter()
                .any(|o| matches!(o.stage, PreferenceStage::Completed) && o.record.entity == entity)
        };
        let failed = Arc::new(tokio::sync::Notify::new());
        *test_hooks::FAIL_NEXT_PREFERENCE_PUBLISH.lock() =
            Some((installation.to_vec(), failed.clone(), None));
        let turns = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let mut worker = idle_worker(alix.context.clone());
        worker.completed_turns = Some(turns.clone());
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        let started = xmtp_common::time::Instant::now();
        alix.set_consent_states(std::slice::from_ref(&a)).await?;
        alix.set_consent_states(std::slice::from_ref(&b)).await?;
        {
            let run = worker.run_internal().fuse();
            let observe = async {
                async fn until(done: impl Fn() -> bool) -> Result<(), xmtp_common::time::Expired> {
                    xmtp_common::time::timeout(WAIT, async {
                        while !done() {
                            xmtp_common::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                }
                until(|| completed("later-b")).await.expect("B completes");
                assert!(!completed("failing-a"), "B must complete before A");
                let ticks = turns[1].load(Ordering::SeqCst);
                until(|| turns[1].load(Ordering::SeqCst) > ticks)
                    .await
                    .expect("periodic turns run while A waits");
                assert!(!completed("failing-a"), "A must wait for its backoff");
                until(|| completed("failing-a"))
                    .await
                    .expect("A is retried");
                assert!(started.elapsed() >= Duration::from_secs(1));
            }
            .fuse();
            futures::pin_mut!(run, observe);
            futures::select! {
                () = observe => {},
                result = run => panic!("worker ended before observed turns: {result:?}"),
            }
        }
        subscription.close();
        alix.close().await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn closing_subscription_stops_worker_with_initial_tick_pending() {
        const WAIT: Duration = Duration::from_secs(1);
        tester!(alix, disable_workers, worker_config: jitter_config());
        let mut worker = idle_worker(alix.context.clone());
        let first = TickControl::new();
        worker.tick_gates.push_back(first.gate);
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        let (result, ()) = xmtp_common::time::timeout(WAIT, async {
            futures::join!(worker.run_internal(), async {
                first
                    .ready
                    .await
                    .expect("actual first tick reaches the gate");
                subscription.close();
            })
        })
        .await
        .expect("closed subscription must stop while the initial tick is pending");
        result?;
        assert!(
            first.release.is_canceled(),
            "ending the worker must drop the pending tick receiver"
        );
        alix.close().await?;
    }
}
