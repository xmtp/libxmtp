use super::{
    DeviceSyncClient, DeviceSyncError, ReceivedContent, decode_sync_message,
    preference_sync::apply_preference_updates, selected_sync_group_in, with_plaintext,
};
use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    identity::IdentityExt,
    identity_updates::{load_identity_updates_for_client, store_association_state},
    subscriptions::{
        incoming::{IncomingCoordinator, IncomingScope},
        internal::{InternalEvent, emit_consent_changes, emit_root_key_changed},
    },
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, NeedsDbReconnect, Worker, WorkerFactory,
        WorkerKind, WorkerResult, metrics::WorkerMetrics,
    },
};
use futures::TryFutureExt;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::OnceCell;
use tracing::instrument;
use xmtp_common::Event;
use xmtp_common::time::Instant;
use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
use xmtp_db::prelude::*;
use xmtp_db::processed_device_sync_messages::SyncMessageKey;
use xmtp_db::{
    StorageError,
    TransactionOutcome::Continue,
    sync_outbox::{OwedPage, clear_owed, has_owed, settle_sent},
};
use xmtp_events::Subscription;
use xmtp_id::associations::{AssociationState, IdentityAction, verify_updates};
use xmtp_macro::log_event;
use xmtp_proto::ShortHex;
use xmtp_proto::xmtp::device_sync::content::{
    DeviceSyncAcknowledge, PreferenceUpdates as PreferenceUpdatesProto,
    device_sync_content::Content as ContentProto,
};

pub(super) fn log_incoming_preference_updates(
    updates: &[xmtp_proto::xmtp::device_sync::content::PreferenceUpdate],
) {
    tracing::info!(update_count = updates.len(), "Incoming preference updates");
}

const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(10 * 60);

/// 2 s doubling to 10 min.
fn backoff(n: u32) -> Duration {
    RETRY_FIRST.saturating_mul(1 << n.min(10)).min(RETRY_MAX)
}

/// A pass that is owed. Nothing is ever dropped: the work is in the database.
#[derive(Default)]
struct Pass {
    due: Option<Instant>,
    failures: u32,
}

impl Pass {
    fn owed_now() -> Self {
        Self {
            due: Some(Instant::now()),
            failures: 0,
        }
    }

    /// A wake does not cut a backoff short.
    fn wake(&mut self) {
        if self.failures == 0 {
            self.due = Some(Instant::now());
        }
    }

    fn is_due(&self) -> bool {
        self.due.is_some_and(|at| at <= Instant::now())
    }

    // implements: SYNC-033
    fn finish(
        &mut self,
        pass: &'static str,
        result: Result<(), DeviceSyncError>,
    ) -> Result<(), DeviceSyncError> {
        match result {
            Ok(()) => *self = Self::default(),
            Err(e) if e.needs_db_reconnect() => return Err(e),
            Err(e) => {
                self.failures = self.failures.saturating_add(1);
                self.due = Some(Instant::now() + backoff(self.failures));
                tracing::warn!(error = %e, pass, failures = self.failures, "device sync pass failed; it stays owed");
            }
        }
        Ok(())
    }
}

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn new(context: Context, metrics: Option<DynMetrics>) -> Self {
        let metrics = metrics
            .and_then(|m| m.as_sync_metrics())
            .unwrap_or(Arc::new(WorkerMetrics::new(context.installation_id())));
        let client = DeviceSyncClient::new(context, metrics.clone());

        Self {
            client,
            subscription: None,
            init: OnceCell::new(),
            metrics,
        }
    }
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        let worker = SyncWorker::new(self.context.clone(), metrics);
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
        Factory { context }
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

        // Keep pass futures off the containing worker poll stack.
        Box::pin(self.run_internal()).await
    }

    /// Two independent passes: a failing receive cannot block the outbox,
    /// and the reverse. A wake marks a pass due; the tick re-reads both.
    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        use futures::StreamExt;
        let (base, jitter) = self
            .client
            .context
            .worker_interval(WorkerKind::DeviceSync, Duration::from_secs(20));
        let mut intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        // Poll events while native startup jitter delays the first tick.
        let mut skip_initial_tick = cfg!(not(target_arch = "wasm32"));
        let subscription = self
            .subscription
            .as_ref()
            .expect("runner installs subscription")
            .clone();
        // A restart drains what is owed.
        let (mut inbox, mut outbox) = (Pass::owed_now(), Pass::owed_now());
        loop {
            if inbox.is_due() {
                let result = Box::pin(self.client.process_sync_group_messages()).await;
                inbox.finish("receive", result)?;
            }
            if outbox.is_due() {
                let result = Box::pin(self.client.publish_outbox()).await;
                outbox.finish("publish", result)?;
            }
            let wait = [inbox.due, outbox.due]
                .into_iter()
                .flatten()
                .min()
                .map(|at| at.saturating_duration_since(Instant::now()));
            tokio::select! {
                event = subscription.next() => {
                    let Some(event) = event else { break; };
                    match event.internal {
                        Some(InternalEvent::MessageStored { is_sync: true, .. }) => inbox.wake(),
                        Some(InternalEvent::OutboxChanged) => outbox.wake(),
                        _ => {}
                    }
                }
                _ = xmtp_common::time::sleep(wait.unwrap_or_default()), if wait.is_some() => {}
                _ = intervals.next() => {
                    if skip_initial_tick {
                        skip_initial_tick = false;
                        continue;
                    }
                    inbox.wake();
                    if has_owed(&self.client.db())? {
                        outbox.wake();
                    }
                },
            }
        }
        Ok(())
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
}

/// The own inbox's installation set after each own identity update, in
/// sequence order, with the update's server time.
struct OwnTimeline {
    steps: Vec<(i64, HashSet<Vec<u8>>)>,
}

enum Sender {
    Member,
    Gone,
    NotYetKnown,
}

impl OwnTimeline {
    /// Load the own inbox's identity updates from the network, then build the
    /// timeline. The load also emits own-installation events, which cycle the
    /// root key on a revocation.
    async fn load(context: &impl XmtpSharedContext) -> Result<Self, DeviceSyncError> {
        let db = context.db();
        let inbox_id = context.inbox_id();
        load_identity_updates_for_client(context, &db, &[inbox_id]).await?;
        let updates = db.get_identity_updates(inbox_id, None, None)?;
        // A cached prefix state is one read.
        let mut states = vec![];
        for update in &updates {
            let Some(cached) = db.read_from_cache(inbox_id, update.sequence_id)? else {
                break;
            };
            states.push(AssociationState::try_from(cached).map_err(StorageError::from)?);
        }
        // The rest are verified in one batch, folded from the last cached
        // state, and cached.
        let rest = &updates[states.len()..];
        if !rest.is_empty() {
            let unverified = rest
                .iter()
                .cloned()
                .map(IdentityExt::to_unverified)
                .collect::<Result<Vec<_>, _>>()
                .map_err(ClientError::from)?;
            let verifier = context.scw_verifier();
            let verified = verify_updates(unverified, &verifier)
                .await
                .map_err(ClientError::from)?;
            for (stored, update) in rest.iter().zip(verified) {
                let state = update
                    .update_state(states.last().cloned(), update.client_timestamp_ns)
                    .map_err(ClientError::from)?;
                store_association_state(context, stored.sequence_id, &state)?;
                states.push(state);
            }
        }
        let steps = updates
            .iter()
            .zip(states)
            .map(|(update, state)| {
                let installations = state.installation_ids().into_iter().collect();
                (update.server_timestamp_ns, installations)
            })
            .collect();
        Ok(Self { steps })
    }

    // implements: SYNC-030
    fn sender_status(&self, own_inbox: &str, msg: &StoredGroupMessage) -> Sender {
        if msg.sender_inbox_id != own_inbox {
            // Sync groups admit only the own inbox already.
            return Sender::Gone;
        }
        let id = &msg.sender_installation_id;
        // Up to the last update, in sequence order, that the backend timed at
        // or before the message. Server times need not grow with sequence.
        let n = self
            .steps
            .iter()
            .rposition(|(at, _)| *at <= msg.sent_at_ns)
            .map_or(0, |last| last + 1);
        if n > 0 && self.steps[n - 1].1.contains(id) {
            Sender::Member
        } else if self.steps[..n].iter().any(|(_, set)| set.contains(id)) {
            Sender::Gone
        } else {
            Sender::NotYetKnown
        }
    }
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    /// Apply every unprocessed sync message. A peer's message is applied when
    /// its sender was an own installation at its envelope time; this
    /// installation's own published message is settled.
    // implements: SYNC-016, SYNC-030
    pub(super) async fn process_sync_group_messages(&self) -> Result<(), DeviceSyncError> {
        let installation_id = self.installation_id();
        let mut after = None;
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
            // The plaintext and its inner encoding can hold an HMAC root key.
            // Both are wiped on every path, including skipped messages.
            let page = with_plaintext(page);
            // Every message in `page` was stored before this load, so the load
            // holds every own identity update the backend timed at or before
            // it, unless a lagging replica serves the read. A failure leaves
            // every row Pending, and the pass backs off.
            let own = OwnTimeline::load(&self.context).await?;
            for (msg, plaintext) in page {
                let result = if msg.sender_installation_id == installation_id.as_slice() {
                    self.settle_own_message(&msg, &plaintext).map(drop)
                } else {
                    match own.sender_status(self.context.inbox_id(), &msg) {
                        Sender::Member => self.apply_message(&msg, &plaintext, None),
                        Sender::Gone => {
                            tracing::warn!(
                                group_id = msg.group_id.short_hex(),
                                "dropping a sync message whose sender was not an own installation when it sent"
                            );
                            self.context
                                .db()
                                .mark_device_sync_msg_as_processed(&msg.id)?;
                            continue;
                        }
                        // Its registration is not loaded yet: read it again later.
                        Sender::NotYetKnown => continue,
                    }
                };
                if let Err(err) = result {
                    // implements: SYNC-016 (a storage error leaves the row Pending for the next pass)
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
                }
            }
        }
    }

    /// Settle a sync message this installation sent: once the backend times
    /// it, apply it like a peer's and settle; once it fails for good, mark it
    /// processed and its values stay owed. Returns whether it is still
    /// unpublished.
    pub(super) fn settle_own_message(
        &self,
        msg: &StoredGroupMessage,
        plaintext: &[u8],
    ) -> Result<bool, DeviceSyncError> {
        match msg.delivery_status {
            DeliveryStatus::Unpublished => return Ok(true),
            DeliveryStatus::Failed => self
                .context
                .db()
                .mark_device_sync_msg_as_processed(&msg.id)?,
            DeliveryStatus::Published => self.apply_message(msg, plaintext, None)?,
        }
        Ok(false)
    }

    /// Apply one sync message like a peer's and mark it processed, in one
    /// transaction. This installation's own published message is also settled
    /// to what its receivers stored, and the marks of `page`, the
    /// page it carried, are cleared. Content this client does not
    /// support, such as older archive transfer content, is marked processed.
    pub(super) fn apply_message(
        &self,
        msg: &StoredGroupMessage,
        plaintext: &[u8],
        page: Option<&OwedPage>,
    ) -> Result<(), DeviceSyncError> {
        let content = decode_sync_message(plaintext);
        let own = msg.sender_installation_id == self.installation_id().as_slice();
        let updates = match &content {
            Some(ReceivedContent(ContentProto::PreferenceUpdates(PreferenceUpdatesProto {
                updates,
            }))) => updates.as_slice(),
            Some(ReceivedContent(ContentProto::Acknowledge(DeviceSyncAcknowledge { .. })))
            | None => &[],
        };
        let msg_type = match &content {
            Some(ReceivedContent(ContentProto::PreferenceUpdates(_))) => "PreferenceUpdates",
            Some(ReceivedContent(ContentProto::Acknowledge(_))) => "Acknowledge",
            None => "Unsupported",
        };
        log_event!(
            Event::DeviceSyncProcessingMessages,
            self.context.installation_id(),
            msg_type,
            external = !own,
            message_id = #msg.id,
            group_id = msg.group_id
        );
        if !own {
            log_incoming_preference_updates(updates);
        }
        // implements: PROC-036
        tracing::info!(update_count = updates.len(), "storing preference updates");
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                // Read in this write, so a sync Welcome commits wholly before
                // or after it.
                let selected = selected_sync_group_in(tx, self.context.inbox_id())?;
                let storage = tx.storage();
                let db = storage.db();
                let applied =
                    apply_preference_updates(&db, self.context.inbox_id(), msg, updates, selected)?;
                if own {
                    settle_sent(&db, updates, msg.sent_at_ns)?;
                }
                if let Some(page) = page {
                    clear_owed(&db, page)?;
                }
                emit_consent_changes(&self.context, events, &db, &applied.consent)?;
                if let Some(marked) = applied.root_key {
                    emit_root_key_changed(events, &db, marked)?;
                }
                db.mark_device_sync_msg_as_processed(&msg.id)?;
                Ok::<_, crate::groups::GroupError>(Continue(()))
            },
        )?;
        for update in updates.iter().filter_map(|update| update.update.as_ref()) {
            if !own {
                self.metrics.increment_metric(match update {
                    super::PreferenceUpdateProto::Consent(_) => SyncMetric::ConsentReceived,
                    super::PreferenceUpdateProto::Hmac(_) => SyncMetric::HmacReceived,
                });
            }
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
}

impl WorkerMetrics<SyncMetric> {
    pub async fn wait_for_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.register_interest(SyncMetric::Init, 1).wait().await
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::tester;

    fn idle_worker<C: XmtpSharedContext + 'static>(context: C) -> SyncWorker<C> {
        SyncWorker::new(context, None)
    }

    /// A failed pass stays owed and waits out its backoff; a wake does not
    /// cut the backoff short; a success clears it.
    // verifies: SYNC-033
    #[xmtp_common::test(unwrap_try = true)]
    fn pass_backs_off_and_stays_owed() {
        let mut pass = Pass::owed_now();
        assert!(pass.is_due());
        pass.finish("test", Err(DeviceSyncError::OutboxPageUnpublished))?;
        assert_eq!(pass.failures, 1);
        assert!(pass.due.is_some(), "a failed pass stays owed");
        assert!(!pass.is_due());
        pass.wake();
        assert!(!pass.is_due(), "a wake does not cut a backoff short");
        pass.finish("test", Err(DeviceSyncError::OutboxPageUnpublished))?;
        assert_eq!(pass.failures, 2);
        assert!(backoff(2) > backoff(1));
        pass.finish("test", Ok(()))?;
        assert_eq!((pass.due, pass.failures), (None, 0));
        pass.wake();
        assert!(pass.is_due());
    }

    /// The sender of a sync message is judged at its envelope time against
    /// the own identity history.
    // verifies: SYNC-030
    #[xmtp_common::test]
    fn sender_is_judged_at_the_envelope_time() {
        let set = |ids: &[u8]| ids.iter().map(|id| vec![*id]).collect::<HashSet<_>>();
        let timeline = OwnTimeline {
            steps: vec![(10, set(&[1])), (20, set(&[1, 2])), (30, set(&[2]))],
        };
        let message = |inbox: &str, installation: u8, sent_at_ns| StoredGroupMessage {
            sender_inbox_id: inbox.into(),
            sender_installation_id: vec![installation],
            sent_at_ns,
            ..crate::test::mock::generate_stored_msg(
                xmtp_proto::types::Cursor(1),
                xmtp_proto::types::GroupId::from([1; 16]),
            )
        };
        // The backend's clock stepped back: 3 is revoked at 60, then an
        // update is timed at 50.
        let stepped_back = OwnTimeline {
            steps: vec![(40, set(&[3])), (60, set(&[4])), (50, set(&[4]))],
        };
        for (at, expected) in [(45, "member"), (55, "gone")] {
            let status = match stepped_back.sender_status("me", &message("me", 3, at)) {
                Sender::Member => "member",
                Sender::Gone => "gone",
                Sender::NotYetKnown => "not yet known",
            };
            assert_eq!(status, expected, "3 at {at} after the step back");
        }
        for (inbox, installation, at, expected) in [
            ("me", 1, 15, "member"),
            // Sent before its revocation at 30: applied, whenever it is processed.
            ("me", 1, 29, "member"),
            ("me", 1, 30, "gone"),
            ("me", 2, 15, "not yet known"),
            ("me", 2, 20, "member"),
            ("me", 3, 40, "not yet known"),
            ("other", 1, 15, "gone"),
        ] {
            let status = match timeline.sender_status("me", &message(inbox, installation, at)) {
                Sender::Member => "member",
                Sender::Gone => "gone",
                Sender::NotYetKnown => "not yet known",
            };
            assert_eq!(status, expected, "{inbox} {installation} at {at}");
        }
    }

    /// A timeline built from the cache and one verified pass gives each
    /// prefix the state the resolver gives it, and caches each one.
    // verifies: SYNC-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn timeline_states_match_each_prefix() {
        use diesel::prelude::*;
        use xmtp_db::{ConnectionExt, schema::association_state::dsl};
        tester!(alix1, disable_workers);
        tester!(alix2, from: alix1, disable_workers);
        tester!(alix3, from: alix1, disable_workers);
        let db = alix1.context.db();
        let inbox_id = alix1.inbox_id();
        load_identity_updates_for_client(&alix1.context, &db, &[inbox_id]).await?;
        let updates = db.get_identity_updates(inbox_id, None, None)?;
        assert!(updates.len() >= 3);
        let mut expected = vec![];
        for update in &updates {
            let state = crate::identity_updates::IdentityUpdates::new(&alix1.context)
                .get_association_state(&db, inbox_id, Some(update.sequence_id))
                .await?;
            let installations: HashSet<_> = state.installation_ids().into_iter().collect();
            expected.push((update.server_timestamp_ns, installations));
        }
        // Only the first prefix stays cached, so the fold starts from it.
        db.raw_query(|conn| {
            diesel::delete(
                dsl::association_state.filter(dsl::sequence_id.gt(updates[0].sequence_id)),
            )
            .execute(conn)
        })?;
        let timeline = OwnTimeline::load(&alix1.context).await?;
        assert_eq!(timeline.steps, expected);
        for update in &updates {
            assert!(db.read_from_cache(inbox_id, update.sequence_id)?.is_some());
        }
        drop((alix2, alix3));
    }

    /// Closing the subscription stops the worker after its owed passes.
    #[xmtp_common::test(unwrap_try = true)]
    async fn closing_subscription_stops_worker() {
        tester!(alix, disable_workers);
        let mut worker = idle_worker(alix.context.clone());
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        subscription.close();
        xmtp_common::time::timeout(Duration::from_secs(60), worker.run_internal()).await??;
        alix.close().await?;
    }

    /// The publish span carries no root key, also when a root key is owed.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn publish_span_omits_hmac_key() {
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let (key, _) = xmtp_db::user_preferences::ensure_root_key(&db)?;
        let key_debug = format!("{:?}", key.as_slice());
        let client = alix.device_sync_client();
        let (result, spans) =
            xmtp_logging::test_logging::with_trace_layer_async(true, client.publish_outbox()).await;
        result?;
        let span = spans
            .iter()
            .find(|span| span.name == "publish_outbox")
            .expect("the outbox exports its span");
        let fields = format!("{:?}", span.attributes);
        assert!(fields.contains("publish_outbox"));
        assert!(
            !fields.contains(&key_debug),
            "the publish span must not export the HMAC root key"
        );
        assert!(!has_owed(&db)?, "the root key was published");
        alix.close().await?;
    }
}
