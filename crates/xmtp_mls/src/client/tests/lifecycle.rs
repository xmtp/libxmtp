use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn should_stream_consent() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let receiver = alix.local_events.subscribe();
    let stream = receiver.stream_consent_updates();
    futures::pin_mut!(stream);

    let group = alix
        .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // first record is denied consent to the group.
    group.update_consent_state(ConsentState::Denied).unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // second is allowing consent for the group
    alix.set_consent_states(&[StoredConsentRecord {
        entity: hex::encode(group.group_id),
        state: ConsentState::Allowed,
        entity_type: ConsentType::ConversationId,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // third allowing consent for bo inbox id
    alix.set_consent_states(&[StoredConsentRecord {
        entity: bo.inbox_id().to_string(),
        entity_type: ConsentType::InboxId,
        state: ConsentState::Allowed,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    // First consent update from creating the group
    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Denied);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::InboxId);
    assert_eq!(item[0].entity, bo.inbox_id());
    assert_eq!(item[0].state, ConsentState::Allowed);
}

/// P3-API-015, MLS-REQ-045: a severed registration wait ends before its deadline.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn registration_visibility_deadline_bounds_a_severed_connection() {
    use crate::client::VisibilityConfirmationOptions;
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;
    toxiproxy_test(async || {
        tester!(alix, proxy, disable_workers);
        alix.wait_for_registration_visible(VisibilityConfirmationOptions::default())
            .await
            .unwrap();
        let outcome = AssertUnwindSafe(async {
            alix.for_each_proxy(async |proxy| proxy.disable().await.unwrap())
                .await;
            let started = xmtp_common::time::Instant::now();
            let result = xmtp_common::time::timeout(
                Duration::from_secs(2),
                alix.wait_for_registration_visible(VisibilityConfirmationOptions {
                    timeout_ms: 250,
                }),
            )
            .await;
            (result, started.elapsed())
        })
        .catch_unwind()
        .await;
        alix.for_each_proxy(async |proxy| proxy.enable().await.unwrap())
            .await;
        let (result, elapsed) = outcome.unwrap();
        assert!(elapsed < Duration::from_secs(2));
        assert!(
            result.unwrap().is_err(),
            "a severed registration read must fail"
        );
    })
    .await;
}

#[xmtp_common::timeout(Duration::from_secs(100))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
// Detection of the black-holed connection comes from the h2 transport keepalive.
// Pin it fast (5s ping / 5s ack) so the failure lands in seconds under nextest,
// whose process-per-test isolation guarantees the pin is read before the
// process-wide config latches. Under a plain `cargo test`, a sibling test may
// latch the library defaults (45s/20s) first and the pin becomes a no-op, so
// the timeout budget also covers their ~65s worst-case detection.
#[cfg(not(target_arch = "wasm32"))]
async fn should_reconnect() {
    unsafe {
        std::env::set_var("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "5");
        std::env::set_var("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS", "5");
    }
    toxiproxy_test(async || {
        let alix = Tester::builder().proxy().build().await;
        let bo = Tester::builder().build().await;

        let start_new_convo = || async {
            bo.create_group_with_members(&[alix.inbox_id().to_string()], None, None)
                .await
                .unwrap()
        };

        let stream = alix.client.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);

        start_new_convo().await;

        let success_res = stream.try_next().await;
        assert!(success_res.is_ok());

        // Black hole the connection for a minute, then reconnect. The test will timeout without the keepalives.
        alix.for_each_proxy(async |p| {
            p.with_timeout("downstream".into(), 60_000, 1.0).await;
        })
        .await;

        start_new_convo().await;

        let should_fail = stream.try_next().await;
        assert!(should_fail.is_err());

        start_new_convo().await;

        alix.for_each_proxy(async |p| {
            p.delete_all_toxics().await.unwrap();
        })
        .await;
        xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

        // stream closes after it gets the broken pipe b/c of blackhole & HTTP/2 KeepAlive
        futures_test::assert_stream_done!(stream);
        xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut new_stream = alix.client.stream_conversations(None, false).await.unwrap();
        let new_res = new_stream.try_next().await;
        assert!(new_res.is_ok());
        assert!(new_res.unwrap().is_some());
    })
    .await
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_stops_workers() {
    tester!(client);
    assert!(
        client.workers.is_running(),
        "worker supervisor must be running before close"
    );

    client.close().await?;

    assert!(
        !client.workers.is_running(),
        "supervisor handle should be taken after close"
    );
    assert!(
        client.context.is_closed(),
        "context closed flag must be set after close"
    );
    assert!(
        client.context.cancellation_token().is_cancelled(),
        "cancellation token must be cancelled after close"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_is_idempotent() {
    tester!(client);
    client.close().await?;
    // second call must return Ok(()) without panic
    client.close().await?;
}

// persistent_db: ephemeral in-memory stores no-op on disconnect, so the
// pool-released assertion only meaningfully tests against a real SQLite
// file. Skipped on WASM where file-backed test stores aren't wired in.
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_disconnects_db() {
    use diesel::RunQueryDsl;
    use diesel::sql_query;

    tester!(client, persistent_db);
    client.close().await?;

    let conn = client.context.store().conn();
    let result = conn.raw_query(|c| sql_query("SELECT 1").execute(c));
    assert!(
        result.is_err(),
        "raw_query after close should surface a ConnectionError; got Ok"
    );
}

#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_cancels_callback_stream() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    tester!(client);

    let closed_flag = Arc::new(AtomicBool::new(false));
    let flag_for_cb = closed_flag.clone();

    let _handle = crate::client::Client::stream_conversations_with_callback(
        Arc::new((*client).clone()),
        None,
        move |_| {},
        move || {
            flag_for_cb.store(true, Ordering::SeqCst);
        },
        false,
    );

    client.close().await?;

    xmtp_common::time::timeout(std::time::Duration::from_secs(1), async {
        while !closed_flag.load(Ordering::SeqCst) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("on_close must fire within 1s of Client::close");
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_after_close_errors() {
    tester!(client);
    client.close().await?;

    let err = client
        .reconnect_db()
        .expect_err("reconnect_db after close must fail");
    assert!(
        matches!(err, crate::client::ClientError::AlreadyClosed),
        "expected ClientError::AlreadyClosed, got {err:?}"
    );
}
