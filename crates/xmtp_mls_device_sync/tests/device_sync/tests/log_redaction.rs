//! Device sync tests: log redaction.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
#[cfg(not(target_arch = "wasm32"))]
impl LogSinkTarget for Capture {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
        self.0.lock().push(record);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn device_sync_creation_logs_omit_full_group_id() {
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    let sync = alix.device_sync_client();
    assert!(sync.primary_sync_group()?.is_none());
    let capture = LogCapture::new(Level::Trace);
    let group = sync
        .get_sync_group()
        .with_subscriber(capture.dispatch())
        .await?;
    let output = capture.output();
    let event = output
        .lines()
        .find(|line| line.contains("Creating sync group:"))
        .expect("group creation emitted its INFO event");
    assert!(
        !event.contains(&hex::encode(group.group_id)),
        "group creation log contains the full sync group ID"
    );
}

// verifies: PROC-036
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn device_sync_logs_omit_message_payload() {
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    xmtp_db::user_preferences::ensure_root_key(&alix.context.db())?;
    let capture = LogCapture::new(Level::Trace);
    alix.device_sync_client()
        .publish_outbox()
        .with_subscriber(capture.dispatch())
        .await?;
    let output = capture.output();
    assert!(
        output.contains("publishing owed preferences"),
        "publish emitted its INFO event"
    );
    assert!(
        !output.contains("\"content\":"),
        "publish span records the message payload"
    );
}

// The same event reaches the JSON destination and the public app log callback.
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
fn incoming_preference_logs_omit_secret_material() {
    use std::sync::Arc;
    use xmtp_logging::{Level, test_logging::LogCapture};

    let key = b"synthetic-private-sync-key-e7a619c3".to_vec();
    let entity = "synthetic-private-consent-identity";
    let updates = vec![
        root_key_update(key.clone(), 9_007_199_254_740_993),
        consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
    ];
    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    tracing::dispatcher::with_default(&capture.dispatch(), || {
        log_incoming_preference_updates(&updates);
    });
    let json = capture.output();
    assert!(json.contains("Incoming preference updates"));
    let records = sink.0.lock();
    assert_eq!(records.len(), 1);
    for sensitive in [
        format!("{key:?}"),
        entity.to_string(),
        "9007199254740993".to_string(),
    ] {
        assert!(
            !json.contains(&sensitive),
            "sensitive sync material reached JSON logging; synthetic JSON: {json}; synthetic app records: {records:?}"
        );
        assert!(
            !records[0].message.contains(&sensitive),
            "sensitive material reached the app log message"
        );
        assert!(
            records[0]
                .fields
                .values()
                .all(|value| !value.contains(&sensitive)),
            "sensitive material reached app log fields"
        );
    }
    assert!(json.contains("\"update_count\":2"));
    assert_eq!(records[0].message, "Incoming preference updates");
    assert_eq!(
        records[0].fields.get("update_count"),
        Some(&"2".to_string())
    );
}

// verifies: PROC-036
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
async fn stored_preference_logs_omit_secret_material() {
    use std::sync::Arc;
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    let client = alix.device_sync_client();
    let group = client.get_sync_group().await?;
    let mut key = b"synthetic-private-sync-key-4bd3e901".to_vec();
    key.resize(42, 0xa7);
    let entity = "synthetic-private-consent-identifier";
    let updates = vec![
        root_key_update(key.clone(), now_ns() + xmtp_common::NS_IN_HOUR),
        consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
    ];
    group
        .send_message(&sync_bytes(updates), SendMessageOpts::default())
        .await?;
    let db = alix.context.db();
    assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);
    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    client
        .process_sync_group_messages()
        .with_subscriber(capture.dispatch())
        .await?;

    assert!(db.unprocessed_sync_group_messages(None)?.is_empty());
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(key.clone().into())
    );
    assert_eq!(
        held(&alix, &ConsentKey::inbox(entity)).unwrap().state,
        ConsentState::Denied
    );
    let json = capture.output();
    let records = sink.0.lock();
    assert!(json.contains("storing preference updates"));
    assert!(
        records
            .iter()
            .any(|record| record.message.contains("storing preference updates"))
    );
    for sensitive in [format!("{key:?}"), hex::encode(&key), entity.into()] {
        assert!(
            !json.contains(&sensitive),
            "sensitive preference data reached JSON"
        );
        assert!(
            records.iter().all(|record| {
                !record.message.contains(&sensitive)
                    && record
                        .fields
                        .values()
                        .all(|value| !value.contains(&sensitive))
            }),
            "sensitive preference data reached the log sink"
        );
    }
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
