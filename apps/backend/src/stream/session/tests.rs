use super::*;
use crate::stream::{OUTBOUND_QUEUE_BYTES, Registry, StreamEnd};
use futures::stream;

const CONFIG: &str =
    "[database]\nurl = 'postgres://localhost/test'\n[server]\nidentifier = 'org.xmtp.test'";

type Queue = Arc<Mutex<mpsc::Receiver<Frame>>>;

/// A native session with one registered topic and no transport, with its
/// registry, mailbox, output queue, registry id, and topic.
fn idle_session(
    config: ValidatedConfig,
) -> (Session, Arc<Registry>, Arc<Mailbox>, Queue, u64, Vec<u8>) {
    let config = Arc::new(config);
    let registry = Arc::new(Registry::default());
    registry.ready();
    let mailbox = Arc::new(Mailbox::default());
    let id = registry.connect(mailbox.clone(), StreamKind::Bidi).unwrap();
    let topic = xmtp_proto::types::TopicKind::WelcomeMessagesV1
        .create([9; 32])
        .to_vec();
    registry.add(id, topic.clone(), 1).unwrap();
    let hub = Arc::new(StreamHub {
        registry: registry.clone(),
        read: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/test")
            .unwrap(),
        fetches: Arc::new(tokio::sync::Semaphore::new(super::super::FETCH_WORKERS)),
        maintenance: Arc::new(tokio::sync::Notify::new()),
        worker: tokio::spawn(pending()),
    });
    let (output, receiver) = mpsc::channel(OUTBOUND_FRAMES);
    let receiver = Arc::new(Mutex::new(receiver));
    let session = Session {
        static_subscription: false,
        hub,
        id,
        request_id: uuid::Uuid::new_v4(),
        config: config.clone(),
        mailbox: mailbox.clone(),
        output,
        queue: receiver.clone(),
        topics: HashMap::new(),
        ready: VecDeque::new(),
        fetching: None,
        update_id: 0,
        pending_update: None,
        deferred_updates: VecDeque::new(),
        deferred_bytes: 0,
        updates: Bucket::new(
            config.limits.max_update_frames_per_second,
            config.limits.max_update_burst,
        ),
        pings: Bucket::new(
            config.limits.max_ping_frames_per_second,
            config.limits.max_ping_burst,
        ),
        send_idle: Instant::now(),
        challenge: None,
        nonce: 0,
        outbound_wait: None,
    };
    (session, registry, mailbox, receiver, id, topic)
}

fn closed(receiver: &Mutex<mpsc::Receiver<Frame>>) -> bool {
    matches!(
        receiver.lock().try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    )
}

// verifies: API-259
#[xmtp_common::test(unwrap_try = true)]
async fn ping_drain_half_close_returns_completion_without_an_error() {
    let (mut session, registry, mailbox, receiver, id, topic) =
        idle_session(ValidatedConfig::load_str(CONFIG)?);

    let (_sent, handed) = tokio::sync::oneshot::channel();
    session.challenge = Some(Challenge {
        nonce: 1,
        handed,
        deadline: Some(Instant::now()),
        stall: Instant::now(),
    });
    let mut input: BoxStream<'static, Result<api::SubscribeRequest, Status>> =
        Box::pin(stream::empty());
    assert!(
        session
            .on_timer(&mut input, &mut VecDeque::new())?
            .is_break()
    );
    assert!(mailbox.terminal.error().is_none());
    drop(session);
    assert!(closed(&receiver));
    assert_eq!(
        registry.add(id, topic, 2).unwrap_err().code(),
        tonic::Code::Unavailable
    );
}

// verifies: API-256, API-259
#[xmtp_common::test(unwrap_try = true)]
async fn ping_drain_half_close_answers_a_buffered_client_ping() {
    let (mut session, registry, mailbox, receiver, id, topic) =
        idle_session(ValidatedConfig::load_str(CONFIG)?);

    let (_sent, handed) = tokio::sync::oneshot::channel();
    session.challenge = Some(Challenge {
        nonce: 1,
        handed,
        deadline: Some(Instant::now()),
        stall: Instant::now(),
    });
    let frames = vec![
        Ok(api::SubscribeRequest {
            request: Some(api::subscribe_request::Request::Update(
                api::subscribe_request::Update {
                    id: 1,
                    adds: vec![],
                    removes: vec![],
                },
            )),
        }),
        Ok(api::SubscribeRequest {
            request: Some(api::subscribe_request::Request::Ping(api::Ping {
                nonce: 92,
            })),
        }),
    ];
    let mut input: BoxStream<'static, Result<api::SubscribeRequest, Status>> =
        Box::pin(stream::iter(frames));
    assert!(
        session
            .on_timer(&mut input, &mut VecDeque::new())?
            .is_break()
    );
    let frame = receiver
        .lock()
        .try_recv()
        .expect("a received client Ping must receive Pong before EOF");
    assert!(matches!(
        frame.value,
        WireResponse::Native(api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Pong(api::Pong {
                nonce: 92
            }))
        })
    ));
    assert!(mailbox.terminal.error().is_none());
    drop(session);
    assert!(closed(&receiver));
    assert_eq!(
        registry.add(id, topic, 2).unwrap_err().code(),
        tonic::Code::Unavailable
    );
}

// verifies: API-251
#[xmtp_common::test(unwrap_try = true)]
async fn buffered_input_rejects_malformed_frames_before_half_close() {
    use crate::test_support::{metrics, query_topic, topic};

    let Some(recorder) = metrics::isolated(
        "stream::session::tests::buffered_input_rejects_malformed_frames_before_half_close",
    ) else {
        return;
    };
    use api::subscribe_request::{Request, Update};

    let valid_topic = topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[4; 32]);
    let valid_query = query_topic(valid_topic.clone(), 0);
    let bad_topic = api::Topic { topic: vec![0] };
    let update = |id, adds, removes| api::SubscribeRequest {
        request: Some(Request::Update(Update { id, adds, removes })),
    };
    let cases = [
        ("unset request", api::SubscribeRequest::default()),
        ("zero id", update(0, vec![], vec![])),
        ("repeated accepted id", update(7, vec![], vec![])),
        ("older id", update(6, vec![], vec![])),
        (
            "missing topic",
            update(9, vec![api::TopicQuery::default()], vec![]),
        ),
        (
            "invalid added topic",
            update(9, vec![query_topic(bad_topic.clone(), 0)], vec![]),
        ),
        ("invalid removed topic", update(9, vec![], vec![bad_topic])),
        (
            "duplicate adds",
            update(9, vec![valid_query.clone(), valid_query.clone()], vec![]),
        ),
        (
            "duplicate removes",
            update(9, vec![], vec![valid_topic.clone(), valid_topic.clone()]),
        ),
        (
            "overlapping topics",
            update(9, vec![valid_query], vec![valid_topic.clone()]),
        ),
        (
            "cursor overflow",
            update(9, vec![query_topic(valid_topic.clone(), u64::MAX)], vec![]),
        ),
        (
            "legal queued capacity",
            update(9, vec![query_topic(valid_topic, 0)], vec![]),
        ),
    ];
    let mut failures = Vec::new();
    for (name, invalid) in cases {
        for deferred in [false, true] {
            let legal_capacity = name == "legal queued capacity";
            let mut config = crate::config::Config::from_toml_str(CONFIG)?;
            if legal_capacity {
                // A valid configuration adds at most as many topics in one
                // update as a stream may hold.
                config.limits.max_stream_topics = 1;
                config.limits.max_update_adds = 1;
            }
            let (mut session, registry, _, receiver, id, topic) = idle_session(config.validate()?);

            session.update_id = 7;
            if legal_capacity {
                session.topics.insert(
                    topic.clone(),
                    Registration {
                        generation: 7,
                        floor: 0,
                        needed: 0,
                        queued: false,
                        fetching: false,
                        acknowledged: true,
                    },
                );
            }
            let first = if legal_capacity {
                update(
                    8,
                    vec![],
                    vec![api::Topic {
                        topic: topic.clone(),
                    }],
                )
            } else {
                update(8, vec![], vec![])
            };
            let mut queued = VecDeque::new();
            let invalid_before = metrics::value(
                &recorder,
                "xmtp_stream_updates_total",
                &[("outcome", "invalid")],
            );
            let result = if deferred {
                session.pending_update = Some(PendingUpdate {
                    id: 7,
                    removed_topics: 0,
                    topics: vec![],
                    heads: pending().boxed(),
                });
                session.input(first)?;
                session.input(invalid.clone())
            } else {
                let (_sent, handed) = tokio::sync::oneshot::channel();
                session.challenge = Some(Challenge {
                    nonce: 1,
                    handed,
                    deadline: Some(Instant::now()),
                    stall: Instant::now(),
                });
                let frames = vec![Ok(first), Ok(invalid.clone())];
                let mut input: BoxStream<'static, Result<api::SubscribeRequest, Status>> =
                    Box::pin(stream::iter(frames));
                session.on_timer(&mut input, &mut queued).map(|_| ())
            };
            let path = if deferred { "deferred" } else { "timer" };
            let invalid_after = metrics::value(
                &recorder,
                "xmtp_stream_updates_total",
                &[("outcome", "invalid")],
            );
            let expected_invalid =
                if !legal_capacity && matches!(&invalid.request, Some(Request::Update(_))) {
                    1.0
                } else {
                    0.0
                };
            if invalid_after - invalid_before != expected_invalid {
                failures.push(format!(
                    "{path} {name}: invalid outcome delta {}, expected {expected_invalid}",
                    invalid_after - invalid_before,
                ));
            }
            if legal_capacity && result.is_err() {
                failures.push(format!(
                    "{path} {name}: legal queued fields failed: {result:?}"
                ));
            } else if !legal_capacity
                && result.as_ref().err().map(Status::code) != Some(tonic::Code::InvalidArgument)
            {
                failures.push(format!(
                    "{path} {name}: expected INVALID_ARGUMENT, got {result:?}"
                ));
            }
            let count = if deferred {
                session.deferred_updates.len()
            } else {
                queued.len()
            };
            if count != if legal_capacity { 2 } else { 1 } {
                failures.push(format!(
                    "{path} {name}: unexpected queued frame count: {count}"
                ));
            }
            assert_eq!(
                session.update_id, 7,
                "validation must not accept queued updates"
            );
            assert_eq!(session.pending_update.is_some(), deferred);
            assert_eq!(session.topics.len(), usize::from(legal_capacity));
            assert!(receiver.lock().try_recv().is_err());
            drop(session);
            assert!(closed(&receiver));
            assert_eq!(
                registry.add(id, topic, 2).unwrap_err().code(),
                tonic::Code::Unavailable
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// verifies: API-257
#[xmtp_common::test(unwrap_try = true)]
async fn queued_ping_ends_the_stream_when_the_transport_never_takes_it() {
    for taken_at_expiry in [false, true] {
        let mut config = crate::config::Config::from_toml_str(CONFIG)?;
        config.streams.max_pong_wait_ms = 20;
        let (mut session, _, mailbox, receiver, ..) = idle_session(config.validate()?);
        let mut input: BoxStream<'static, Result<api::SubscribeRequest, Status>> =
            Box::pin(stream::pending());
        let before = Instant::now();
        assert!(
            session
                .on_timer(&mut input, &mut VecDeque::new())?
                .is_continue()
        );
        assert!(session.challenge.as_ref().unwrap().stall >= before + Duration::from_millis(20));
        // The nonce is a counter: a Pong before handoff must not answer the Ping.
        session.input(api::SubscribeRequest {
            request: Some(api::subscribe_request::Request::Pong(api::Pong {
                nonce: session.nonce,
            })),
        })?;
        assert!(session.challenge.is_some());
        xmtp_common::time::timeout(Duration::from_secs(5), session.timer()).await?;
        if taken_at_expiry {
            let task = tokio::spawn(pending());
            let mut output = SessionOutput::new(receiver, mailbox.terminal.clone(), task);
            assert!(output.next().await.unwrap().is_ok());
            assert!(
                session
                    .on_timer(&mut input, &mut VecDeque::new())?
                    .is_continue()
            );
            assert!(session.challenge.as_ref().unwrap().deadline.is_some());
        } else {
            let error = session
                .on_timer(&mut input, &mut VecDeque::new())
                .unwrap_err();
            assert_eq!(error.code(), tonic::Code::DeadlineExceeded);
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn failed_session_frees_queued_output_without_a_transport_poll() {
    for failed in [false, true] {
        let (mut session, _, mailbox, receiver, ..) =
            idle_session(ValidatedConfig::load_str(CONFIG)?);
        for nonce in 0..3 {
            session.control(api::subscribe_response::Response::Pong(api::Pong { nonce }))?;
        }
        if failed {
            mailbox
                .terminal
                .fail(Status::deadline_exceeded("pong"), StreamEnd::Keepalive);
        }
        drop(session);
        assert_eq!(
            mailbox.budget.available() == OUTBOUND_QUEUE_BYTES,
            failed,
            "failed: {failed}"
        );
        let delivered = std::iter::from_fn(|| receiver.lock().try_recv().ok()).count();
        assert_eq!(delivered, if failed { 0 } else { 3 });
    }
}
