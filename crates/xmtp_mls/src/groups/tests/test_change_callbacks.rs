//! Coverage for the unstable group-change callbacks
//! ([`crate::groups::change_callbacks`]).

use crate::groups::change_callbacks::{
    AppDataChange, AppDataChangeCallback, UnstableChangeCallbacks,
};
use crate::groups::intents::{QueueIntent, UpdateMetadataIntentData};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use crate::utils::TestMlsGroup;
use std::sync::{Arc, Mutex, OnceLock};
use xmtp_db::group_intent::{IntentState, StoredGroupIntent};
use xmtp_db::prelude::*;

/// Records every change it is handed, so a test can assert on both the fact of
/// delivery and the payload.
#[derive(Default)]
struct RecordingCallback {
    changes: Mutex<Vec<AppDataChange>>,
}

impl RecordingCallback {
    fn recorded(&self) -> Vec<AppDataChange> {
        self.changes.lock().expect("lock poisoned").clone()
    }
}

#[xmtp_common::async_trait]
impl AppDataChangeCallback for RecordingCallback {
    async fn on_app_data_changed(&self, change: AppDataChange) {
        self.changes.lock().expect("lock poisoned").push(change);
    }
}

fn recording() -> (Arc<RecordingCallback>, UnstableChangeCallbacks) {
    let callback = Arc::new(RecordingCallback::default());
    let callbacks = UnstableChangeCallbacks {
        app_data: Some(callback.clone() as Arc<dyn AppDataChangeCallback>),
    };
    (callback, callbacks)
}

/// A remote member's `app_data` update reaches the receiver's callback with
/// both sides of the change — the case a semantic merge is built on.
#[xmtp_common::test(unwrap_try = true)]
async fn test_app_data_callback_fires_for_remote_change() {
    let (recorder, callbacks) = recording();
    tester!(alix);
    tester!(bo, change_callbacks: callbacks);

    let group = alix.create_group(None, None).await?;
    group.add_members(&[bo.inbox_id()]).await?;
    group.update_app_data("from alix".to_string(), None).await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id).await?;
    bo_group.sync().await?;

    let recorded = recorder.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "expected exactly one app_data change, got {recorded:?}"
    );
    assert_eq!(recorded[0].group_id, group.group_id.to_vec());
    assert_eq!(recorded[0].new_value.as_deref(), Some("from alix"));
    // The welcome carries alix's pre-add state, so bo's first *processed*
    // change starts from the empty slot the group was created with.
    assert_eq!(recorded[0].old_value.as_deref(), Some(""));
}

/// The callback also observes changes this client made itself — an
/// implementation that reacts by writing has to be idempotent, so the contract
/// is pinned here rather than left to chance.
#[xmtp_common::test(unwrap_try = true)]
async fn test_app_data_callback_fires_for_local_change() {
    let (recorder, callbacks) = recording();
    tester!(alix, change_callbacks: callbacks);

    let group = alix.create_group(None, None).await?;
    group.update_app_data("from alix".to_string(), None).await?;

    let recorded = recorder.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "expected exactly one app_data change, got {recorded:?}"
    );
    assert_eq!(recorded[0].new_value.as_deref(), Some("from alix"));
}

/// Messages that leave `app_data` alone must not wake the callback — a
/// reconciler woken by every chat message would be worse than none.
#[xmtp_common::test(unwrap_try = true)]
async fn test_app_data_callback_silent_for_unrelated_changes() {
    let (recorder, callbacks) = recording();
    tester!(alix);
    tester!(bo, change_callbacks: callbacks);

    let group = alix.create_group(None, None).await?;
    group.add_members(&[bo.inbox_id()]).await?;
    group.update_group_name("renamed".to_string()).await?;
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id).await?;
    bo_group.sync().await?;

    assert!(
        recorder.recorded().is_empty(),
        "app_data callback fired for a non-app_data change: {:?}",
        recorder.recorded()
    );
}

/// Publishing a merged value straight back into the same group is the whole
/// point of the callback, and it is also the one thing that can deadlock:
/// `update_app_data` waits on `sync_until_intent_resolved`, which re-enters
/// `sync_with_conn` and takes the per-group sync mutex. Dispatching while sync
/// still held that mutex hung the caller forever.
///
/// Wrapped in a timeout so a regression fails the run instead of parking it
/// until the CI job's own limit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_callback_can_publish_back_into_the_same_group() {
    /// Reacts to the peer's write by publishing a merge. The merge is echoed
    /// back to this same callback, so it must react exactly once or it would
    /// chase itself.
    #[derive(Default)]
    struct RepublishingCallback {
        group: OnceLock<TestMlsGroup>,
        published: Mutex<Vec<String>>,
    }

    #[xmtp_common::async_trait]
    impl AppDataChangeCallback for RepublishingCallback {
        async fn on_app_data_changed(&self, change: AppDataChange) {
            if change.new_value.as_deref() != Some("from alix") {
                return;
            }
            let group = self.group.get().expect("group registered before sync");
            let merged = "from alix + from bo".to_string();
            group
                .update_app_data(merged.clone(), None)
                .await
                .expect("republish from callback");
            self.published.lock().expect("lock poisoned").push(merged);
        }
    }

    let callback = Arc::new(RepublishingCallback::default());
    let callbacks = UnstableChangeCallbacks {
        app_data: Some(callback.clone() as Arc<dyn AppDataChangeCallback>),
    };

    tester!(alix);
    tester!(bo, change_callbacks: callbacks);

    let alix_group = alix.create_group(None, None).await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id).await?;
    callback
        .group
        .set(bo_group.clone())
        .map_err(|_| "group already set")
        .unwrap();

    alix_group
        .update_app_data("from alix".to_string(), None)
        .await?;

    xmtp_common::time::timeout(std::time::Duration::from_secs(30), bo_group.sync())
        .await
        .expect("sync deadlocked: the callback was dispatched while sync held the group mutex")?;

    assert_eq!(
        callback.published.lock().expect("lock poisoned").as_slice(),
        ["from alix + from bo".to_string()],
        "callback should have published its merge exactly once"
    );
    assert_eq!(bo_group.app_data().await?, "from alix + from bo");

    alix_group.sync().await?;
    assert_eq!(alix_group.app_data().await?, "from alix + from bo");
}

/// A published-but-unresolved local `update_app_data` is a blind absolute set:
/// `UpdateMetadataIntentData` freezes the value at queue time, and an intent
/// that loses an epoch race is rebuilt from that same frozen data and
/// republished. So a remote change that lands in between is reported to the
/// host and then silently overwritten by the local intent.
///
/// This pins the hazard rather than endorsing it — a host reconciling
/// `app_data` must treat the callback as "state moved", not "state settled",
/// and re-assert its merge after its own in-flight write lands.
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_local_intent_clobbers_a_remote_change() {
    let (recorder, callbacks) = recording();
    tester!(alix, change_callbacks: callbacks);
    tester!(bo);

    let alix_group = alix.create_group(None, None).await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    alix_group
        .update_app_data("start".to_string(), None)
        .await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id).await?;
    bo_group.sync().await?;

    // Bo commits a change alix has not seen yet.
    bo_group
        .update_app_data("from-bo".to_string(), None)
        .await?;

    // Alix, still on the pre-bo view, queues and publishes its own absolute
    // set. The intent is now Published and unresolved.
    let intent_data: Vec<u8> =
        UpdateMetadataIntentData::new_update_app_data("from-alix".to_string(), None).into();
    QueueIntent::metadata_update()
        .data(intent_data)
        .queue(&alix_group).await?;
    alix_group.publish_intents().await?;

    // First sync applies bo's commit and bounces the losing intent back to
    // ToPublish; the second lets it republish on the new epoch.
    alix_group.sync().await?;
    alix_group.sync().await?;

    let values: Vec<_> = recorder
        .recorded()
        .into_iter()
        .filter_map(|c| c.new_value)
        .collect();

    // Alix is told about bo's value, and then about its own write replacing it.
    assert!(
        values.contains(&"from-bo".to_string()),
        "expected the remote change to be reported, got {values:?}"
    );
    assert_eq!(
        values.last().map(String::as_str),
        Some("from-alix"),
        "expected the stale local intent to land last, got {values:?}"
    );
    assert_eq!(
        alix_group.app_data().await?,
        "from-alix",
        "the frozen local intent overwrites the remote value"
    );
}

/// The same race as [`test_pending_local_intent_clobbers_a_remote_change`],
/// but with a compare-and-swap guard. The guard is re-checked on every publish
/// attempt — including the republish after the intent loses the epoch race —
/// so the stale write is abandoned instead of overwriting bo's value.
#[xmtp_common::test(unwrap_try = true)]
async fn test_guarded_update_is_abandoned_instead_of_clobbering() {
    let (recorder, callbacks) = recording();
    tester!(alix, change_callbacks: callbacks);
    tester!(bo);

    let alix_group = alix.create_group(None, None).await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    alix_group
        .update_app_data("start".to_string(), None)
        .await?;

    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id).await?;
    bo_group.sync().await?;

    bo_group
        .update_app_data("from-bo".to_string(), None)
        .await?;

    // Alix publishes a guarded write derived from the pre-bo value.
    let intent_data: Vec<u8> = UpdateMetadataIntentData::new_update_app_data(
        "from-alix".to_string(),
        Some("start".to_string()),
    )
    .into();
    let queued = QueueIntent::metadata_update()
        .data(intent_data)
        .queue(&alix_group).await?;
    alix_group.publish_intents().await?;

    alix_group.sync().await?;
    alix_group.sync().await?;

    assert_eq!(
        alix_group.app_data().await?,
        "from-bo",
        "the guard must abandon the stale write rather than overwrite"
    );
    // Abandoning is only half the contract — the intent must land in a state
    // the caller can tell apart from success, or `update_app_data` reports a
    // silent no-op.
    let intent: Option<StoredGroupIntent> =
        Fetch::<StoredGroupIntent>::fetch(&alix.context.db(), &queued.id).await?;
    assert_eq!(
        intent.map(|i| i.state),
        Some(IntentState::Superseded),
        "an abandoned guarded intent must be Superseded, not Processed"
    );
    let values: Vec<_> = recorder
        .recorded()
        .into_iter()
        .filter_map(|c| c.new_value)
        .collect();
    assert_eq!(
        values.last().map(String::as_str),
        Some("from-bo"),
        "the host's last observed value should be bo's, got {values:?}"
    );
}

/// The synchronous pre-flight: a guard that is already stale when the caller
/// asks fails immediately with a typed error carrying the value that actually
/// landed, so the host can re-derive without a network round trip.
#[xmtp_common::test(unwrap_try = true)]
async fn test_guarded_update_reports_the_value_that_landed() {
    tester!(alix);

    let group = alix.create_group(None, None).await?;
    group.update_app_data("current".to_string(), None).await?;

    let err = group
        .update_app_data("next".to_string(), Some("stale".to_string()))
        .await
        .expect_err("a stale guard must not be applied");

    match err {
        crate::groups::GroupError::AppDataSuperseded { expected, actual } => {
            assert_eq!(expected, "stale");
            assert_eq!(actual, "current");
        }
        other => panic!("expected AppDataSuperseded, got {other:?}"),
    }
    assert_eq!(group.app_data().await?, "current");
}

/// A superseded intent must terminate `sync_until_intent_resolved` promptly
/// with an error. If the state were not treated as terminal the call would
/// spin until the sync retry budget expired, turning a lost race into a long
/// hang and then a misleading timeout.
#[xmtp_common::test(unwrap_try = true)]
async fn test_superseded_intent_resolves_promptly_as_an_error() {
    tester!(alix);

    let group = alix.create_group(None, None).await?;
    group.update_app_data("current".to_string(), None).await?;

    // Queue a guarded write whose guard is already stale, bypassing the
    // synchronous pre-flight so the publish-time check is what fires.
    let intent_data: Vec<u8> = UpdateMetadataIntentData::new_update_app_data(
        "next".to_string(),
        Some("stale".to_string()),
    )
    .into();
    let queued = QueueIntent::metadata_update()
        .data(intent_data)
        .queue(&group).await?;

    let result = group.sync_until_intent_resolved(queued.id).await;
    assert!(
        result.is_err(),
        "a superseded intent must not resolve as success"
    );

    let intent: Option<StoredGroupIntent> =
        Fetch::<StoredGroupIntent>::fetch(&alix.context.db(), &queued.id).await?;
    assert_eq!(intent.map(|i| i.state), Some(IntentState::Superseded));
    assert_eq!(
        group.app_data().await?,
        "current",
        "the guarded write must not have been applied"
    );
}
