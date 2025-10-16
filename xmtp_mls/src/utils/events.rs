use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::device_sync::DeviceSyncError,
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};
use std::{
    fmt::Debug,
    sync::{LazyLock, atomic::Ordering},
};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_common::time::now_ns;
use xmtp_configuration::DeviceSyncUrls;
use xmtp_db::{
    DbQuery, StorageError, Store,
    events::{EVENTS_ENABLED, EventLevel, Events},
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

pub(crate) struct EventBuilder<'a, E> {
    pub event: E,
    pub details: Option<serde_json::Value>,
    pub group_id: Option<&'a [u8]>,
    pub level: Option<EventLevel>,
    pub icon: Option<String>,
}

impl<'a, E> EventBuilder<'a, E>
where
    E: AsRef<str>,
{
    pub fn new(event: E) -> Self {
        Self {
            event,
            details: None,
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
/// ```rust,ignore
/// use xmtp_mls::track;
/// track!("user_login", {
///     "timestamp": "2024-01-01T12:00:00Z",
///     "method": "oauth"
/// });
/// ```
///
/// Track a message event with details:
/// ```rust,ignore
/// use xmtp_mls::track;
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
/// ```rust,ignore
/// use xmtp_mls::track;
/// track!("group_created", {
///     "name": "Team Chat",
///     "member_count": 5
/// }, group_id: group.id());
/// ```
///
/// Track an event with custom level:
/// ```rust,ignore
/// use xmtp_mls::track;
/// track!("error_occurred", {
///     "error_type": "network_timeout",
///     "retry_count": 3
/// }, level: EventLevel::Error);
/// ```
///
/// Track an event with both group and level:
/// ```rust,ignore
/// use xmtp_mls::track;
/// use xmtp_db::events::EventLevel;
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
    ($label:expr, $details:tt $(, $k:ident $(: $v:expr)?)*) => {{
        let details = serde_json::json!($details);
        track!($label, details: details $(, $k $(: $v)?)*)
    }};

    ($label:expr $(, $k:ident $(: $v:expr)?)*) => {{
        #[allow(unused_mut)]
        let mut builder = $crate::utils::events::EventBuilder::new($label.to_string());
        track!(@process builder $(, $k $(: $v)?)*)
    }};

    (@process $builder:ident) => {
        $builder.track()
    };

    (@process $builder:ident, details: $details:expr $(, $k:ident $(: $v:expr)?)*) => {{
        $builder.details = Some($details);
        track!(@process $builder $(, $k $(: $v)?)*)
    }};

    (@process $builder:ident, level: $level:expr $(, $k:ident $(: $v:expr)?)*) => {{
        $builder.level = Some($level);

        if matches!($level, EventLevel::Fault) {
            $builder.icon = Some("☠️".to_string());
        }

        track!(@process $builder $(, $k $(: $v)?)*)
    }};

    (@process $builder:ident, icon: $icon:literal $(, $k:ident $(: $v:expr)?)*) => {{
        $builder.icon = Some($icon.to_string());
        track!(@process $builder $(, $k $(: $v)?)*)
    }};
    (@process $builder:ident, group: $group:expr $(, $k:ident $(: $v:expr)?)*) => {
        track!(@process $builder, group_id: $group $(, $k $(: $v)?)*)
    };
    (@process $builder:ident, group_id: $group_id:expr $(, $k:ident $(: $v:expr)?)*) => {{
        $builder.group_id = Some($group_id);
        track!(@process $builder $(, $k $(: $v)?)*)
    }};
    (@process $builder:ident, maybe_group_id: $maybe_group_id:expr $(, $k:ident $(: $v:expr)?)*) => {
        $builder.group_id = $maybe_group_id;
        track!(@process $builder $(, $k $(: $v)?)*)
    };

    (@process $builder:ident, result: $result:expr $(, $k:ident $(: $v:expr)?)*) => {{
        match $result {
            Ok(_) => {
                track!(
                    @process $builder,
                    level: xmtp_db::events::EventLevel::Success
                    $(, $k $(: $v)?)*
                )
            },
            Err(err) => {
                $builder.event = format!("Error: {}", $builder.event);
                let mut details = serde_json::json!({
                        "error": format!("{err:?}"),
                        "location": format!("{}: {}", file!(), line!()),
                });
                if let Some(existing) = &$builder.details {
                    json_patch::merge(&mut details, existing);
                };

                track!(
                    @process $builder,
                    details: details,
                    level: xmtp_db::events::EventLevel::Error,
                    icon: "⚠️"
                    $(, $k $(: $v)?)*
                )
            }
        };
    }};
}

/// This macro inspects a `Result` value and automatically tracks an error event if the result
/// contains an `Err` variant, using the underlying `track!` macro. The original result is
/// returned unchanged, making this completely transparent to control flow.
///
/// # Usage
///
/// ```rust,ignore
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
    ($name:literal, $result:expr $(, $k:ident $(: $v:expr)?)*) => {{
        let result = &$result;
        if result.is_err() {
            track!($name, result: result $(, $k $(: $v)?)*)
        }
    }};
}

#[macro_export]
macro_rules! track_request {
    ($name:literal, $details:tt $(, $k:ident $(: $v:expr)?)*) => {
        track!($name, $details, icon: "🛜" $(, $k $(: $v)?)*)
    };
}

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
{
    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(EventWorker::new(self.context.clone()));
        (worker, metrics)
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::Event
    }
}

pub struct EventWorker<Context> {
    rx: broadcast::Receiver<Events>,
    context: Context,
}

impl<Context> EventWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub(crate) fn new(context: Context) -> Self {
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
impl<Context> Worker for EventWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + Send + Sync + 'static,
    {
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
    db: impl DbQuery + Send + Sync + 'static,
    device_sync_server_url: Option<impl AsRef<str>>,
) -> Result<String, DeviceSyncError> {
    let device_sync_server_url = device_sync_server_url
        .map(|url| url.as_ref().to_string())
        .unwrap_or(DeviceSyncUrls::PRODUCTION_ADDRESS.to_string());

    let options = BackupOptions {
        elements: vec![BackupElementSelection::Event as i32],
        ..Default::default()
    };

    // Generate a random encryption key
    let key = xmtp_common::rand_vec::<32>();

    // Build the exporter
    let exporter = ArchiveExporter::new(options, db, &key);

    let url = format!("{device_sync_server_url}/upload");
    let response = exporter.post_to_url(&url).await?;

    Ok(format!(
        "{device_sync_server_url}/client-events?key={response}_{}",
        hex::encode(key)
    ))
}

#[cfg(test)]
mod tests {
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::{tester, utils::events::upload_debug_archive};
    use std::time::Duration;
    use xmtp_configuration::DeviceSyncUrls;

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_debug_pkg() {
        tester!(alix, stream);
        tester!(bo);
        tester!(caro);

        let (bo_dm, _msg) = bo.test_talk_in_dm_with(&alix).await?;

        let alix_dm = alix.group(&bo_dm.group_id)?;
        alix_dm
            .send_message(b"Hello there", SendMessageOpts::default())
            .await?;
        xmtp_common::time::sleep(Duration::from_millis(1000)).await;
        alix_dm
            .send_message(b"Hello there", SendMessageOpts::default())
            .await?;

        caro.test_talk_in_dm_with(&alix).await?;
        alix.sync_welcomes().await?;

        let g = alix
            .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
            .await?;
        g.update_group_name("Group with the buds".to_string())
            .await?;
        g.send_message(b"Hello there", SendMessageOpts::default())
            .await?;
        g.sync().await?;

        bo.sync_welcomes().await?;
        let bo_g = bo.group(&g.group_id)?;
        bo_g.send_message(b"Gonna add Caro", SendMessageOpts::default())
            .await?;
        bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

        caro.sync_welcomes().await?;
        let caro_g = caro.group(&g.group_id)?;
        caro_g
            .send_message(b"hi guise!", SendMessageOpts::default())
            .await?;

        g.sync().await?;

        let k = upload_debug_archive(alix.db(), Some(DeviceSyncUrls::LOCAL_ADDRESS)).await?;
        tracing::info!("{k}");

        // Exported and uploaded no problem
    }
}
