use super::*;
use tokio::sync::{Mutex, mpsc};
use xmtp_common::StreamHandle;
use xmtp_logging::SinkQueue;

async fn receive<T>(receiver: &mut mpsc::UnboundedReceiver<T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(3), receiver.recv())
        .await
        .expect("log callback did not complete")
        .expect("log callback channel closed")
}

struct Host {
    queue: std::sync::Weak<SinkQueue<dyn LogSink>>,
    entered: mpsc::UnboundedSender<LogRecord>,
    release: Mutex<mpsc::UnboundedReceiver<bool>>,
}

#[xmtp_common::async_trait]
impl LogSink for Host {
    async fn log(&self, record: LogRecord) -> Result<(), LogSinkError> {
        if !self.queue.upgrade().unwrap().handoff() {
            return Ok(());
        }
        self.entered.send(record).unwrap();
        if self.release.lock().await.recv().await.unwrap() {
            Ok(())
        } else {
            Err(LogSinkError::Failed {
                reason: "app rejected record".into(),
            })
        }
    }
}

fn record(index: usize) -> xmtp_logging::LogRecord {
    xmtp_logging::LogRecord {
        level: xmtp_logging::Level::Info,
        target: "xmtp_sdk".into(),
        message: index.to_string(),
        fields: Default::default(),
        timestamp_ns: 0,
        dropped_records: 0,
    }
}

// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-009, LOG-013
#[xmtp_common::test(unwrap_try = true)]
async fn asynchronous_drain_retains_failed_drop_report() {
    let queue: Arc<SinkQueue<dyn LogSink>> = Arc::default();
    let (entered, mut records) = mpsc::unbounded_channel();
    let (release, responses) = mpsc::unbounded_channel();
    queue.replace(Some(Arc::new(Host {
        queue: Arc::downgrade(&queue),
        entered,
        release: Mutex::new(responses),
    })));
    let mut task = xmtp_common::spawn(None, drain(queue.clone()));
    queue.push(record(0));
    assert_eq!(receive(&mut records).await.message, "0");
    let emitter = queue.clone();
    std::thread::spawn(move || {
        for index in 1..=xmtp_logging::SINK_QUEUE_CAPACITY + 3 {
            emitter.push(record(index));
        }
    })
    .join()
    .unwrap();
    assert_eq!(queue.dropped_count(), 3);
    release.send(true)?;
    assert_eq!(receive(&mut records).await.dropped_records, 3);
    release.send(false)?;
    assert_eq!(receive(&mut records).await.dropped_records, 3);
    release.send(true)?;
    assert_eq!(receive(&mut records).await.dropped_records, 0);
    queue.replace(None);
    release.send(true)?;
    assert!(matches!(
        task.end_and_wait().await,
        Err(xmtp_common::StreamHandleError::Cancelled)
    ));
}

struct Reentrant {
    queue: std::sync::Weak<SinkQueue<dyn LogSink>>,
    finished: mpsc::UnboundedSender<()>,
}

#[xmtp_common::async_trait]
impl LogSink for Reentrant {
    async fn log(&self, _: LogRecord) -> Result<(), LogSinkError> {
        let queue = self.queue.upgrade().unwrap();
        assert!(queue.handoff());
        queue.push(record(1));
        queue.replace(None);
        self.finished.send(()).unwrap();
        Ok(())
    }
}

// verifies: LOG-007, LOG-008, LOG-011
#[xmtp_common::test(unwrap_try = true)]
async fn callback_can_emit_and_clear_itself() {
    let queue: Arc<SinkQueue<dyn LogSink>> = Arc::default();
    let (finished, mut completion) = mpsc::unbounded_channel();
    queue.replace(Some(Arc::new(Reentrant {
        queue: Arc::downgrade(&queue),
        finished,
    })));
    let mut task = xmtp_common::spawn(None, drain(queue.clone()));
    queue.push(record(0));
    receive(&mut completion).await;
    assert!(matches!(
        task.end_and_wait().await,
        Err(xmtp_common::StreamHandleError::Cancelled)
    ));
}

struct QueueCapture(Arc<SinkQueue<dyn LogSink>>);
impl xmtp_logging::LogSinkTarget for QueueCapture {
    fn on_record(&self, record: xmtp_logging::LogRecord) -> Result<(), xmtp_logging::SinkError> {
        self.0.push(record);
        Ok(())
    }
}

// verifies: LOG-001, LOG-010
#[xmtp_common::test(unwrap_try = true)]
async fn client_log_secrets_are_redacted() {
    use tracing::instrument::WithSubscriber;
    let credential = "LOG_CREDENTIAL_SENTINEL_89d42";
    let signing_key = b"LOG_SIGNING_KEY_SENTINEL_89d42!!!";
    let encryption_key = b"LOG_DATABASE_KEY_SENTINEL_89d42";
    assert_eq!(signing_key.len(), 33);
    assert_eq!(encryption_key.len(), 31);
    let queue: Arc<SinkQueue<dyn LogSink>> = Arc::default();
    let (entered, mut records) = mpsc::unbounded_channel();
    let (release, responses) = mpsc::unbounded_channel();
    queue.replace(Some(Arc::new(Host {
        queue: Arc::downgrade(&queue),
        entered,
        release: Mutex::new(responses),
    })));
    let capture = xmtp_logging::test_logging::LogCapture::with_sink(
        xmtp_logging::Level::Trace,
        Some(Arc::new(QueueCapture(queue.clone()))),
    );
    let mut task = xmtp_common::spawn(None, drain(queue.clone()));
    async {
        let result = crate::Backend::connect(crate::BackendOptions {
            url: "http://127.0.0.1:1".into(),
            credential: Some(crate::Credential {
                name: None,
                value: format!("Bearer {credential}\n"),
                expires_at_seconds: i64::MAX,
            }),
            ..Default::default()
        })
        .await;
        assert!(result.is_err());
        assert!(
            crate::local_signer_from_private_key(signing_key.to_vec())
                .await
                .is_err()
        );
        assert!(
            crate::client::open_store(
                &crate::StorageOptions {
                    encryption_key: Some(encryption_key.to_vec()),
                    ..Default::default()
                },
                None
            )
            .await
            .is_err()
        );
        tracing::error!(target: "xmtp_sdk::conformance", "secret operations completed");
    }
    .with_subscriber(capture.dispatch())
    .await;
    let mut app_records = Vec::new();
    loop {
        let record = receive(&mut records).await;
        let last = record.message == "secret operations completed";
        app_records.push(record);
        release.send(true)?;
        if last {
            break;
        }
    }
    let native = capture.output();
    let app = format!("{app_records:?}");
    assert!(native.contains("secret operations completed"));
    for secret in [
        credential,
        std::str::from_utf8(signing_key)?,
        std::str::from_utf8(encryption_key)?,
    ] {
        let native_exposed = native.contains(secret);
        let app_exposed = app.contains(secret);
        assert!(
            !native_exposed && !app_exposed,
            "secret exposure: native={native_exposed}, app={app_exposed}"
        );
    }
    for secret in [signing_key.as_slice(), encryption_key.as_slice()] {
        let native_exposed = native.contains(&format!("{secret:?}"));
        let app_exposed = app.contains(&format!("{secret:?}"));
        assert!(
            !native_exposed && !app_exposed,
            "key exposure: native={native_exposed}, app={app_exposed}"
        );
    }
    queue.replace(None);
    let _ = task.end_and_wait().await;
}
