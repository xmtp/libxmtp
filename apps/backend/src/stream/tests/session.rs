use crate::test_support as support;
use support::{TestServer, native::{Native, envelope}};
use crate::api::{self, subscribe_response::Response as Frame, subscribe_request::Request as Input};
use tonic::Code;

#[xmtp_common::test(unwrap_try = true)]
async fn empty_session_acknowledges_updates_and_ping_then_ends_on_half_close() {
    let server = TestServer::new(|_| {}).await?;
    let mut stream = Native::open(&server).await?;
    stream.update(1, vec![], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(applied) if applied.id == 1 && applied.added_targets.is_empty()));
    stream.send(Input::Ping(api::Ping { nonce: 77 })).await?;
    assert!(matches!(stream.next().await?, Frame::Pong(pong) if pong.nonce == 77));
    drop(stream.input);
    assert!(stream.output.message().await?.is_none());
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn fixed_target_history_hands_off_to_live_in_order() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let metas = server.publish((0..100).map(|value| envelope(1, value)).collect()).await?;
    let topic = metas[0].topic.clone().unwrap();
    let mut stream = Native::open(&server).await?;
    stream.update(1, vec![support::query_topic(topic.clone(), 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(applied) if applied.added_targets[0].through_sequence_id == 100));
    let later = server.publish(vec![envelope(1, 100)]).await?;
    let rows = stream.messages(101).await?;
    assert_eq!(rows.iter().map(|row| row.meta.as_ref().unwrap().cursor.as_ref().unwrap().sequence_id).collect::<Vec<_>>(), (1..=101).collect::<Vec<_>>());
    stream.update(2, vec![support::query_topic(topic, 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(applied) if applied.added_targets.is_empty()));
    assert_eq!(rows.last().unwrap().meta, Some(later[0].clone()));
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn future_cursor_filters_history_and_future_rows_below_its_floor() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let mut stream = Native::open(&server).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[2; 32]);
    stream.update(1, vec![support::query_topic(topic, 3)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(applied) if applied.added_targets[0].through_sequence_id == 0));
    server.publish((0..4).map(|value| envelope(2, value)).collect()).await?;
    let rows = stream.messages(1).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].meta.as_ref().unwrap().cursor.as_ref().unwrap().sequence_id, 4);
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn removal_acknowledgement_separates_old_and_new_registrations() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let metas = server.publish((0..200).map(|value| envelope(3, value)).collect()).await?;
    let topic = metas[0].topic.clone().unwrap();
    let mut stream = Native::open(&server).await?;
    let mut blocked_history = server.backend.store.primary.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN ACCESS EXCLUSIVE MODE").execute(&mut *blocked_history).await?;
    stream.update(1, vec![support::query_topic(topic.clone(), 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(_)));
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database()
            AND wait_event_type = 'Lock' AND query LIKE 'SELECT wanted.ordinal%') AS "waiting!""#)
            .fetch_one(&server.backend.store.primary).await.unwrap()
    }, true).await?;
    stream.update(2, vec![], vec![topic.clone()]).await?;
    loop { if matches!(stream.next().await?, Frame::Applied(applied) if applied.id == 2) { break; } }
    stream.send(Input::Ping(api::Ping { nonce: 9 })).await?;
    assert!(matches!(stream.next().await?, Frame::Pong(pong) if pong.nonce == 9));
    stream.update(3, vec![support::query_topic(topic, 199)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(applied) if applied.id == 3));
    blocked_history.commit().await?;
    let rows = stream.messages(1).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].meta.as_ref().unwrap().cursor.as_ref().unwrap().sequence_id, 200);
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn structural_update_errors_close_the_session() {
    let server = TestServer::new(|_| {}).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[4; 32]);
    for request in [
        api::SubscribeRequest::default(),
        api::SubscribeRequest { request: Some(Input::Update(api::subscribe_request::Update { id: 0, adds: vec![], removes: vec![] })) },
        api::SubscribeRequest { request: Some(Input::Update(api::subscribe_request::Update { id: 1, adds: vec![support::query_topic(topic.clone(), 0)], removes: vec![topic.clone()] })) },
        api::SubscribeRequest { request: Some(Input::Update(api::subscribe_request::Update { id: 1, adds: vec![support::query_topic(topic.clone(), u64::MAX)], removes: vec![] })) },
    ] {
        let mut stream = Native::open(&server).await?;
        stream.input.send(request).await?;
        assert_eq!(stream.output.message().await.unwrap_err().code(), Code::InvalidArgument);
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn unmatched_server_challenge_expires_despite_other_inbound_traffic() {
    let server = TestServer::new(|config| { config.streams.keepalive_interval_ms = 20; config.streams.max_pong_wait_ms = 60; }).await?;
    let mut stream = Native::open(&server).await?;
    let Frame::Ping(challenge) = stream.next().await? else { panic!("expected server challenge"); };
    stream.send(Input::Pong(api::Pong { nonce: challenge.nonce + 1 })).await?;
    stream.send(Input::Ping(api::Ping { nonce: 88 })).await?;
    assert!(matches!(stream.next().await?, Frame::Pong(pong) if pong.nonce == 88));
    assert_eq!(stream.output.message().await.unwrap_err().code(), Code::DeadlineExceeded);
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn ping_and_update_use_independent_buckets() {
    let server = TestServer::new(|config| { config.limits.max_ping_burst = 2; config.limits.max_update_burst = 2; config.limits.max_ping_frames_per_second = 1; config.limits.max_update_frames_per_second = 1; }).await?;
    let mut stream = Native::open(&server).await?;
    for id in 1..=2 {
        stream.update(id, vec![], vec![]).await?;
        assert!(matches!(stream.next().await?, Frame::Applied(_)));
        stream.send(Input::Ping(api::Ping { nonce: id })).await?;
        assert!(matches!(stream.next().await?, Frame::Pong(_)));
    }
    stream.send(Input::Ping(api::Ping { nonce: 3 })).await?;
    assert_eq!(stream.output.message().await.unwrap_err().code(), Code::ResourceExhausted);
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn one_connection_request_pool_remains_available_while_tailer_runs() {
    let server = TestServer::new(|config| { config.database.max_connections = 1; config.streams.poll_interval_ms = 10; }).await?;
    let mut stream = Native::open(&server).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[31; 32]);
    stream.update(1, vec![support::query_topic(topic, 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(_)));
    let meta = server.publish(vec![envelope(31, 1)]).await?.remove(0);
    assert_eq!(stream.messages(1).await?[0].meta, Some(meta));
    drop(stream);
    server.stop().await?;
}

fn large(topic: u8, value: u8) -> api::ClientEnvelope {
    let mut envelope = envelope(topic, value);
    if let Some(api::client_envelope::Payload::WelcomeMessage(welcome)) = &mut envelope.payload
        && let Some(api::welcome_message::Version::V1(welcome)) = &mut welcome.version { welcome.data.resize(900_000, value); }
    envelope
}

#[xmtp_common::test(unwrap_try = true)]
async fn byte_cutoff_preserves_unvisited_topic_priority() {
    let server = TestServer::new(|_| {}).await?;
    server.publish((0..4).map(|value| large(32, value)).chain((0..4).map(|value| large(33, value))).collect()).await?;
    let a = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[32; 32]);
    let b = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[33; 32]);
    let mut stream = Native::open(&server).await?;
    stream.update(1, vec![support::query_topic(a.clone(), 0), support::query_topic(b.clone(), 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(_)));
    let Frame::Messages(first) = stream.next().await? else { panic!("expected first history turn"); };
    let Frame::Messages(second) = stream.next().await? else { panic!("expected second history turn"); };
    assert_eq!(first.envelopes.len(), 2);
    assert_eq!(second.envelopes.len(), 2);
    assert!(first.envelopes.iter().all(|row| row.meta.as_ref().unwrap().topic == Some(a.clone())));
    assert!(second.envelopes.iter().all(|row| row.meta.as_ref().unwrap().topic == Some(b.clone())));
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn applied_control_can_exceed_the_data_frame_target() {
    use prost::Message;
    let server = TestServer::new(|_| {}).await?;
    let mut stream = Native::open(&server).await?;
    let adds = (0_u64..100_000).map(|id| {
        let mut group = [0; 16];
        group[..8].copy_from_slice(&id.to_be_bytes());
        support::query_topic(support::topic(xmtp_proto::types::TopicKind::GroupMessagesV1, &group), 0)
    }).collect();
    stream.update(1, adds, vec![]).await?;
    let Frame::Applied(applied) = stream.next().await? else { panic!("expected all targets"); };
    assert_eq!(applied.added_targets.len(), 100_000);
    assert!(applied.encoded_len() > 2 * 1024 * 1024);
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn slow_live_consumer_fails_without_blocking_other_sessions() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let mut slow = Native::open(&server).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[34; 32]);
    slow.update(1, vec![support::query_topic(topic, 0)], vec![]).await?;
    assert!(matches!(slow.next().await?, Frame::Applied(_)));
    let mut healthy = Native::open(&server).await?;
    server.publish((0..24).map(|value| large(34, value)).collect()).await?;
    healthy.send(Input::Ping(api::Ping { nonce: 91 })).await?;
    assert!(matches!(healthy.next().await?, Frame::Pong(pong) if pong.nonce == 91));
    loop {
        match slow.output.message().await {
            Ok(Some(_)) => {},
            Err(error) => { assert_eq!(error.code(), Code::ResourceExhausted); break; },
            Ok(None) => panic!("slow stream ended without a capacity error"),
        }
    }
    drop(slow);
    drop(healthy);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn pending_target_capture_does_not_block_ping_or_half_close() {
    let server = TestServer::new(|_| {}).await?;
    let mut stream = Native::open(&server).await?;
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query!("LOCK TABLE topic_watermark IN ACCESS EXCLUSIVE MODE").execute(&mut *blocker).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[35; 32]);
    stream.update(1, vec![support::query_topic(topic, 0)], vec![]).await?;
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database()
            AND wait_event_type = 'Lock' AND query LIKE 'SELECT COALESCE%') AS "waiting!""#).fetch_one(&server.backend.store.primary).await.unwrap()
    }, true).await?;
    stream.send(Input::Ping(api::Ping { nonce: 92 })).await?;
    assert!(matches!(stream.next().await?, Frame::Pong(pong) if pong.nonce == 92));
    drop(stream.input);
    assert!(stream.output.message().await?.is_none());
    blocker.commit().await?;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn unknown_gap_capacity_fails_before_discarding_recovery_state() {
    let server = TestServer::new(|config| { config.streams.max_gap_ranges = 1; config.streams.poll_interval_ms = 10; config.publishing.max_barrier_wait_ms = 20; }).await?;
    let mut stream = Native::open(&server).await?;
    let topic = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[36; 32]);
    stream.update(1, vec![support::query_topic(topic, 0)], vec![]).await?;
    assert!(matches!(stream.next().await?, Frame::Applied(_)));
    let mut barrier = server.backend.store.primary.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock_shared(0, 2)").execute(&mut *barrier).await?;
    for value in 0..2 {
        let mut aborted = server.backend.store.primary.begin().await?;
        sqlx::query!("SELECT pg_advisory_xact_lock_shared(0, 2)").execute(&mut *aborted).await?;
        sqlx::query!("SELECT nextval('envelope_sequence')").fetch_one(&mut *aborted).await?;
        aborted.rollback().await?;
        server.publish(vec![envelope(36, value)]).await?;
    }
    loop {
        let response = xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), stream.output.message()).await?;
        match response {
            Err(error) => { assert_eq!(error.code(), Code::ResourceExhausted); break; },
            Ok(Some(_)) => {},
            Ok(None) => panic!("missing explicit gap capacity failure"),
        }
    }
    assert_eq!(sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton").fetch_one(&server.backend.store.primary).await?, 0);
    barrier.commit().await?;
    drop(stream);
    server.stop().await?;
}
