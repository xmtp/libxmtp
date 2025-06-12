use crate::{
    client::ClientError,
    context::{XmtpMlsLocalContext, XmtpSharedContext},
    groups::device_sync::DeviceSyncError,
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};
use serde::Serialize;
use std::{
    fmt::Debug,
    sync::{atomic::Ordering, Arc, LazyLock},
};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_api::XmtpApi;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_common::time::now_ns;
use xmtp_db::{
    events::{EventLevel, Events, EVENTS_ENABLED},
    StorageError, Store, XmtpDb, XmtpOpenMlsProvider,
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

#[derive(Debug, Error)]
pub enum EventError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}
impl NeedsDbReconnect for EventError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

static EVENT_TX: LazyLock<broadcast::Sender<Events>> = LazyLock::new(|| broadcast::channel(100).0);

pub(crate) struct EventBuilder<'a, E, D> {
    pub event: E,
    pub details: D,
    pub group_id: Option<&'a [u8]>,
    pub level: Option<EventLevel>,
    pub icon: Option<String>,
}

impl<'a, E, D> EventBuilder<'a, E, D>
where
    E: AsRef<str>,
    D: Serialize,
{
    pub fn new(event: E, details: D) -> Self {
        Self {
            event,
            details,
            group_id: None,
            level: None,
            icon: None,
        }
    }

    fn build(self) -> Result<Events, serde_json::Error> {
        Ok(Events {
            created_at_ns: now_ns(),
            details: serde_json::to_value(&self.details)?,
            event: self.event.as_ref().to_string(),
            group_id: self.group_id.map(|g| g.to_vec()),
            level: self.level.unwrap_or(EventLevel::None),
            icon: self.icon,
        })
    }

    pub(crate) fn track(self) {
        if !EVENTS_ENABLED.load(Ordering::Relaxed) {
            return;
        }

        let event = match self.build() {
            Ok(event) => event,
            Err(err) => {
                tracing::warn!("Unable to track event: {err:?}");
                return;
            }
        };

        if let Err(err) = EVENT_TX.send(event) {
            tracing::warn!("Unable to send event to writing worker: {err:?}");
        }
    }
}

/// A convenient macro for tracking events in the XMTP system.
///
/// This macro provides a flexible way to create and track events with details and optional metadata
/// such as group association and event level. Events are automatically timestamped and
/// serialized before being sent to the event processing worker.
///
/// # Basic Usage
///
/// The macro requires an event name and details object as the first two arguments:
///
/// ```rust
/// track!("user_login", {
///     "timestamp": "2024-01-01T12:00:00Z",
///     "method": "oauth"
/// });
/// ```
///
/// Track a message event with details:
/// ```rust
/// track!("message_sent", {
///     "recipient": "alice@example.com",
///     "message_type": "text",
///     "size_bytes": 1024
/// });
/// ```
///
/// # Required Arguments
///
/// 1. **Event name** - A string literal or expression identifying the event type
/// 2. **Details** - A JSON object containing event-specific data
///
/// # Optional Parameters
///
/// The macro supports several optional parameters that can be specified in any order:
///
/// - `group_id: <expr>` - Associates the event with a specific group ID
/// - `group: <expr>` - Alias for `group_id` for convenience
/// - `level: <expr>` - Sets the event level (see `EventLevel` enum)
///
/// # Examples
///
/// Track an event with group association:
/// ```rust
/// track!("group_created", {
///     "name": "Team Chat",
///     "member_count": 5
/// }, group_id: group.id());
/// ```
///
/// Track an event with custom level:
/// ```rust
/// track!("error_occurred", {
///     "error_type": "network_timeout",
///     "retry_count": 3
/// }, level: EventLevel::Error);
/// ```
///
/// Track an event with both group and level:
/// ```rust
/// track!("message_delivery_failed", {
///     "reason": "recipient_offline",
///     "will_retry": true
/// }, group: group_id, level: EventLevel::Warning);
/// ```
///
/// # Implementation Details
///
/// - Events are automatically timestamped with nanosecond precision
/// - Details are serialized to JSON using `serde_json`
/// - Events are sent asynchronously to an event worker for processing
/// - If event tracking fails (e.g., serialization error), a warning is logged but execution continues
/// - The macro is designed to be non-blocking and failure-safe
///
/// # Error Handling
///
/// The macro handles errors gracefully:
/// - Serialization errors are logged as warnings
/// - Channel send errors (if the event worker is unavailable) are logged as warnings
/// - The calling code continues execution regardless of tracking success/failure
#[macro_export]
macro_rules! track {
    ($label:expr) => {
        track!($label, (serde_json::json!(())))
    };
    ($label:literal, $details:tt $(, $k:ident $(: $v:expr)?)*) => {
        track!(($label.to_string()), $details $(, $k $(: $v)?)*)
    };
    ($label:expr, $details:tt $(, $k:ident $(: $v:expr)?)*) => {
        let details = serde_json::json!($details);
        #[allow(unused_mut)]
        let mut builder = $crate::utils::events::EventBuilder::new($label, details);
        track!(@process builder $(, $k $(: $v)?)*)
    };

    (@process $builder:expr) => {
        $builder.track();
    };

    (@process $builder:expr, group: $group:expr $(, $k:ident $(: $v:expr)?)*) => {
        track!(@process $builder, group_id: $group $(, $k $(: $v)?)*)
    };
    (@process $builder:expr, group_id: $group_id:expr $(, $k:ident $(: $v:expr)?)*) => {
        $builder.group_id = Some($group_id);
        track!(@process $builder $(, $k $(: $v)?)*)
    };
    (@process $builder:expr, maybe_group_id: $maybe_group_id:expr $(, $k:ident $(: $v:expr)?)*) => {
        $builder.group_id = $maybe_group_id;
        track!(@process $builder $(, $k $(: $v)?)*)
    };

    (@process $builder:expr, level: $level:expr $(, $k:ident $(: $v:expr)?)*) => {
        $builder.level = Some($level);
        track!(@process $builder $(, $k $(: $v)?)*)
    };

    (@process $builder:expr, icon: $icon:literal $(, $k:ident $(: $v:expr)?)*) => {
        $builder.icon = Some($icon.to_string());
        track!(@process $builder $(, $k $(: $v)?)*)
    };
}

/// This macro inspects a `Result` value and automatically tracks an error event if the result
/// contains an `Err` variant, using the underlying `track!` macro. The original result is
/// returned unchanged, making this completely transparent to control flow.
///
/// # Usage
///
/// ```rust
/// // Track with default "Error" label
/// let result = track_err!(some_operation());
///
/// // Track with custom label
/// let result = track_err!(database_query(), label: "db_failed");
///
/// // With additional context (supports all track! macro parameters)
/// let result = track_err!(api_call(),
///     label: "api_timeout",
///     group_id: &group_id
/// );
/// ```
///
/// The error is stored in the event details as `{"error": "Debug representation"}`.
/// See the `track!` macro documentation for all available parameters and options.
#[macro_export]
macro_rules! track_err {
    ($result:expr, label: $label:expr $(, $k:ident $(: $v:expr)?)*) => {{
        let result = $result;
        if let Err(err) = &result {
            track!(
                $label,
                {
                    "error": format!("{err:?}"),
                    "location": format!("{}: {}", file!(), line!()),
                    "wrapped": stringify!($result)
                },
                level: xmtp_db::events::EventLevel::Error,
                icon: "🚨"
                $(, $k $(: $v)?)*
            );
        }
        result
    }};
    ($result:expr $(, $k:ident $(: $v:expr)?)*) => {
        track_err!($result, label: "Error" $(, $k $(: $v)?)*)
    };
}

#[derive(Clone)]
pub struct Factory<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> WorkerFactory for Factory<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(EventWorker::new(self.context.clone())) as Box<_>;
        (worker, metrics)
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::Event
    }
}

pub struct EventWorker<ApiClient, Db> {
    rx: broadcast::Receiver<Events>,
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> EventWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static + Send,
{
    pub(crate) fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        let rx = EVENT_TX.subscribe();
        Self { context, rx }
    }
    async fn run(&mut self) -> Result<(), EventError> {
        while let Ok(event) = self.rx.recv().await {
            event.store(&self.context.db())?;
        }
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<ApiClient, Db> Worker for EventWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext,
        <C as XmtpSharedContext>::Db: 'static,
        <C as XmtpSharedContext>::ApiClient: 'static,
    {
        let context = context.context_ref().clone();
        Factory { context }
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::Event
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().await.map_err(|e| Box::new(e) as Box<_>)
    }
}

pub async fn upload_debug_archive(
    provider: &Arc<XmtpOpenMlsProvider>,
    device_sync_server_url: impl AsRef<str>,
) -> Result<String, DeviceSyncError> {
    let provider = provider.clone();
    let device_sync_server_url = device_sync_server_url.as_ref();

    let options = BackupOptions {
        elements: vec![BackupElementSelection::Event as i32],
        ..Default::default()
    };

    // Generate a random encryption key
    let key = xmtp_common::rand_vec::<32>();

    // Build the exporter
    let exporter = ArchiveExporter::new(options, provider.clone(), &key);

    let url = format!("{device_sync_server_url}/upload");
    let response = exporter.post_to_url(&url).await?;

    Ok(format!("{response}:{}", hex::encode(key)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{configuration::DeviceSyncUrls, tester, utils::events::upload_debug_archive};

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_debug_pkg() {
        tester!(alix, stream);
        tester!(bo);
        tester!(caro);

        let (bo_dm, _msg) = bo.test_talk_in_dm_with(&alix).await?;

        let alix_dm = alix.group(&bo_dm.group_id)?;
        alix_dm.send_message(b"Hello there").await?;
        xmtp_common::time::sleep(Duration::from_millis(1000)).await;
        alix_dm.send_message(b"Hello there").await?;

        caro.test_talk_in_dm_with(&alix).await?;
        alix.sync_welcomes().await?;

        let g = alix
            .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
            .await?;
        g.update_group_name("Group with the buds".to_string())
            .await?;
        g.send_message(b"Hello there").await?;
        g.sync().await?;

        bo.sync_welcomes().await?;
        let bo_g = bo.group(&g.group_id)?;
        bo_g.send_message(b"Gonna add Caro").await?;
        bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

        caro.sync_welcomes().await?;
        let caro_g = caro.group(&g.group_id)?;
        caro_g.send_message(b"hi guise!").await?;

        g.sync().await?;

        let k = upload_debug_archive(&alix.provider, DeviceSyncUrls::LOCAL_ADDRESS).await?;
        tracing::info!("{k}");

        // Exported and uploaded no problem
    }
}
