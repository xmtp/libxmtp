use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn abort_closes_finished_signal_and_releases_dispatcher_lock() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let sender = FakeSender::blocked();
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, 1).await?;
    hub.abort();
    timeout(Duration::from_secs(2), hub.finished()).await?;
    xmtp_common::wait_for_eq(|| holder_pid(&fixture.store), None).await?;
    assert_eq!(fixture.cursor().await, 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_shorter_deadline_interrupts_an_active_drain() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let sender = Arc::new(FakeSender {
        gate: Some(Arc::new(Semaphore::new(0))),
        outcomes: Mutex::new(VecDeque::from([Outcome::Transient { retry_after: None }])),
        ..FakeSender::default()
    });
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, 1).await?;
    hub.stop(Instant::now() + Duration::from_secs(30));
    sender.gate.as_ref()?.add_permits(1);
    // The gated retry starts only after the worker has processed the first
    // stop signal. It remains active when the deadline is shortened.
    xmtp_common::wait_for_eq(|| async { sender.count() }, 2).await?;
    hub.stop(Instant::now());
    timeout(Duration::from_secs(2), hub.finished()).await?;
    assert_eq!(fixture.cursor().await, 1);
    assert_eq!(sender.count(), 2);
}

// verifies: PUSH-257
#[xmtp_common::test(unwrap_try = true)]
async fn only_settled_envelopes_send_and_above_boundary_requests_maintenance() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(2, &[1], false).await?;
    fixture.boundary(1).await?;
    let maintenance = Arc::new(Notify::new());
    let sender = Arc::new(FakeSender::default());
    let hub = fixture.hub_with(sender.clone(), sender.clone(), maintenance.clone());
    timeout(Duration::from_secs(2), maintenance.notified()).await?;
    xmtp_common::wait_for_eq(|| async { sender.count() }, 1).await?;
    assert_eq!(sender.calls.lock()[0].0, 1);
    fixture.boundary(2).await?;
    xmtp_common::wait_for_eq(|| async { sender.count() }, 2).await?;
    stop(&hub).await;
    assert_eq!(fixture.cursor().await, 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_later_window_on_another_channel_runs_while_https_is_stalled() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture
        .recipient(2, PushChannel::Apns, &[2], false, 0)
        .await?;
    fixture.seed(1024, &[1], false).await?;
    fixture.seed(1, &[2], false).await?;
    fixture.boundary(1025).await?;
    let http = FakeSender::blocked();
    let other = Arc::new(FakeSender::default());
    let hub = fixture.hub_with(http.clone(), other.clone(), Arc::new(Notify::new()));
    xmtp_common::wait_for_eq(|| async { other.count() }, 1).await?;
    assert_eq!(http.count(), work::CHANNEL_PERMITS);
    assert_eq!(fixture.cursor().await, 0);
    http.gate.as_ref()?.add_permits(1024);
    stop(&hub).await;
    assert_eq!(fixture.cursor().await, 1025);
}

#[xmtp_common::test(unwrap_try = true)]
async fn drain_waits_for_first_attempts_and_restart_sends_no_duplicate() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(2, &[1], false).await?;
    fixture.boundary(2).await?;
    let sender = FakeSender::blocked();
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, 2).await?;
    hub.stop(Instant::now() + Duration::from_secs(2));
    assert!(
        timeout(Duration::from_millis(100), hub.finished())
            .await
            .is_err()
    );
    sender.gate.as_ref()?.add_permits(2);
    // A cancelled wait must not detach the dispatcher task.
    xmtp_common::wait_for_eq(|| fixture.cursor(), 2).await?;
    let restarted = Arc::new(FakeSender::default());
    let next = fixture.hub(restarted.clone());
    xmtp_common::wait_for_some(|| holder_pid(&fixture.store)).await?;
    assert!(
        timeout(
            Duration::from_millis(100),
            xmtp_common::wait_for_eq(|| async { restarted.count() }, 1)
        )
        .await
        .is_err()
    );
    stop(&next).await;
}

// verifies: PUSH-257
#[xmtp_common::test(unwrap_try = true)]
async fn expired_drain_replays_a_window_with_unfinished_first_attempts() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1100, &[1], false).await?;
    fixture.boundary(1100).await?;
    let sender = FakeSender::blocked();
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, work::CHANNEL_PERMITS).await?;
    hub.stop(Instant::now() + Duration::from_millis(150));
    timeout(Duration::from_secs(1), hub.finished()).await?;
    assert_eq!(fixture.cursor().await, 0);
    let successor = Arc::new(FakeSender::default());
    let next = fixture.hub(successor.clone());
    xmtp_common::wait_for_eq(|| async { successor.count() }, 1100).await?;
    stop(&next).await;
    assert_eq!(fixture.cursor().await, 1100);
}

// verifies: PUSH-257
#[xmtp_common::test(unwrap_try = true)]
async fn terminated_lock_session_stops_loading_and_successor_takes_over() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let first = FakeSender::blocked();
    let left = fixture.hub(first.clone());
    xmtp_common::wait_for_eq(|| async { first.count() }, 1).await?;
    let pid = holder_pid(&fixture.store).await?;
    let second = Arc::new(FakeSender::default());
    let right = fixture.hub(second.clone());
    assert!(
        timeout(
            Duration::from_millis(100),
            xmtp_common::wait_for_eq(|| async { second.count() }, 1)
        )
        .await
        .is_err()
    );
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(&fixture.store.primary)
        .await?;
    xmtp_common::wait_for_eq(|| async { second.count() }, 1).await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(2).await?;
    xmtp_common::wait_for_eq(|| async { second.count() }, 2).await?;
    assert_eq!(first.count(), 1);
    first.gate.as_ref()?.add_permits(1);
    left.stop(Instant::now());
    left.finished().await;
    stop(&right).await;
    assert_eq!(fixture.cursor().await, 2);
}

// verifies: PUSH-226
#[xmtp_common::test(unwrap_try = true)]
async fn expiry_removes_stale_recipients_and_their_subscriptions() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture
        .recipient(2, PushChannel::Http, &[1], false, 0)
        .await?;
    sqlx::query("UPDATE push_recipient SET renewed_ns = 0 WHERE recipient_id = decode(lpad('1',64,'0'),'hex')").execute(&fixture.store.primary).await?;
    let hub = fixture.hub(Arc::new(FakeSender::default()));
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM push_recipient")
                .fetch_one(&fixture.store.primary)
                .await
                .unwrap()
        },
        1,
    )
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_subscription")
        .fetch_one(&fixture.store.primary)
        .await?;
    assert_eq!(count, 1);
    stop(&hub).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn real_https_retries_gone_then_success_without_deleting_recipient() {
    let fixture = Fixture::new().await?;
    let mut webhook = channel::http::tests::Webhook::start(vec![404, 200]).await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    sqlx::query("UPDATE push_recipient SET delivery = $1")
        .bind(&webhook.url)
        .execute(&fixture.store.primary)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let hub = fixture.hub(webhook.sender.clone());
    timeout(Duration::from_secs(2), webhook.requests.recv()).await??;
    let first = Instant::now();
    timeout(Duration::from_secs(3), webhook.requests.recv()).await??;
    assert!(first.elapsed() >= Duration::from_millis(900));
    stop(&hub).await;
    assert_eq!(fixture.cursor().await, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_recipient")
        .fetch_one(&fixture.store.primary)
        .await?;
    assert_eq!(count, 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn real_https_all_gone_deletes_recipient_after_max_attempts() {
    let fixture = Fixture::new().await?;
    let mut webhook = channel::http::tests::Webhook::start(vec![404, 410, 404]).await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    sqlx::query("UPDATE push_recipient SET delivery = $1")
        .bind(&webhook.url)
        .execute(&fixture.store.primary)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let hub = fixture.hub(webhook.sender.clone());
    for _ in 0..3 {
        timeout(Duration::from_secs(3), webhook.requests.recv()).await??;
    }
    stop(&hub).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_recipient")
        .fetch_one(&fixture.store.primary)
        .await?;
    assert_eq!(count, 0);
}

// verifies: PUSH-257
#[xmtp_common::test(unwrap_try = true)]
async fn crash_releases_lock_and_replays_the_unfinished_window() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(2, &[1], false).await?;
    fixture.boundary(2).await?;
    let blocked = FakeSender::blocked();
    let hub = fixture.hub(blocked.clone());
    xmtp_common::wait_for_eq(|| async { blocked.count() }, 2).await?;
    assert_eq!(fixture.cursor().await, 0);
    drop(hub);
    let successor = Arc::new(FakeSender::default());
    let next = fixture.hub(successor.clone());
    xmtp_common::wait_for_eq(|| async { successor.count() }, 2).await?;
    stop(&next).await;
    assert_eq!(fixture.cursor().await, 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn drain_finishes_a_queued_retry_and_loads_no_new_window() {
    let fixture = Fixture::new().await?;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let sender = Arc::new(FakeSender {
        gate: Some(Arc::new(Semaphore::new(0))),
        outcomes: Mutex::new(VecDeque::from([
            Outcome::Transient { retry_after: None },
            Outcome::Delivered,
        ])),
        ..Default::default()
    });
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, 1).await?;
    // Stop before the first outcome creates its retry. Hold both attempts so
    // their phase does not depend on database or CI scheduling speed.
    let drain_budget = channel::ATTEMPT_TIMEOUT * 3;
    hub.stop(Instant::now() + drain_budget);
    sender.gate.as_ref().unwrap().add_permits(1);
    xmtp_common::wait_for_eq(|| fixture.cursor(), 1).await?;
    xmtp_common::wait_for_eq(|| async { sender.count() }, 2).await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(2).await?;
    sender.gate.as_ref().unwrap().add_permits(1);
    timeout(drain_budget, hub.finished()).await?;
    assert_eq!(
        sender
            .calls
            .lock()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert!(sender.outcomes.lock().is_empty());
    assert_eq!(fixture.cursor().await, 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn provider_timeout_completes_first_attempt_and_releases_the_cursor() {
    let mut fixture = Fixture::new().await?;
    fixture.config.push.max_attempts = 1;
    fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.boundary(1).await?;
    let sender = FakeSender::blocked();
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| async { sender.count() }, 1).await?;
    assert_eq!(fixture.cursor().await, 0);
    timeout(
        channel::ATTEMPT_TIMEOUT + Duration::from_secs(2),
        xmtp_common::wait_for_eq(|| fixture.cursor(), 1),
    )
    .await??;
    stop(&hub).await;
    assert_eq!(sender.count(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn pruning_between_pages_does_not_leave_an_incomplete_window_forever() {
    let fixture = Fixture::new().await?;
    sqlx::query("INSERT INTO push_recipient (recipient_id, secret_hash, channel, delivery, signing_key, topic_count, renewed_ns) SELECT decode(lpad(to_hex(id),64,'0'),'hex'), decode(repeat('01',32),'hex'), 3, 'https://push.invalid', decode(repeat('07',32),'hex'), 1, (extract(epoch FROM clock_timestamp()) * 1000000000)::bigint FROM generate_series(1,13000) id")
        .execute(&fixture.store.primary).await?;
    sqlx::query("INSERT INTO push_subscription (recipient_id, topic, since_sequence_id, include_commits) SELECT recipient_id, $1, 0, false FROM push_recipient")
        .bind(&[1u8][..]).execute(&fixture.store.primary).await?;
    fixture
        .recipient(20000, PushChannel::Apns, &[2], false, 0)
        .await?;
    fixture.seed(1, &[1], false).await?;
    fixture.seed(1, &[2], false).await?;
    fixture.boundary(2).await?;
    let blocked = FakeSender::blocked();
    let other = Arc::new(FakeSender::default());
    let hub = fixture.hub_with(blocked.clone(), other.clone(), Arc::new(Notify::new()));
    xmtp_common::wait_for_eq(|| async { blocked.count() }, work::CHANNEL_PERMITS).await?;
    sqlx::query("DELETE FROM envelopes WHERE sequence_id = 1")
        .execute(&fixture.store.primary)
        .await?;
    blocked.gate.as_ref()?.add_permits(13000);
    xmtp_common::wait_for_eq(|| fixture.cursor(), 2).await?;
    assert_eq!(other.count(), 1);
    stop(&hub).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn server_drain_interrupts_blocked_push_queries_within_the_shared_budget() {
    let mut server = crate::test_support::TestServer::new(|config| {
        config.push.http = Some(crate::config::push::HttpConfig::default());
        config.streams.poll_interval_ms = 10;
        config.server.max_drain_duration_ms = 100;
    })
    .await?;
    xmtp_common::wait_for_some(|| holder_pid(&server.backend.store)).await?;
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query("LOCK TABLE envelopes IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await?;
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND (query LIKE '%push_eligible%' OR query LIKE '%window_rows%'))")
            .fetch_one(&server.backend.store.primary).await.unwrap()
    }, true).await?;
    server.shutdown();
    timeout(Duration::from_millis(500), server.wait_stopped()).await??;
    blocker.rollback().await?;
    assert!(holder_pid(&server.backend.store).await.is_none());
    server.stop().await?;
}
