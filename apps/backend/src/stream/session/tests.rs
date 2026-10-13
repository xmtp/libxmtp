use super::*;
use crate::stream::Registry;
use futures::stream;

// verifies: API-259
#[xmtp_common::test(unwrap_try = true)]
async fn ping_drain_half_close_returns_completion_without_an_error() {
    let config: Config = toml::from_str(
        "[database]\nurl = 'postgres://localhost/test'\n[server]\nidentifier = 'org.xmtp.test'",
    )?;
    let config = Arc::new(config);
    let registry = Arc::new(Registry::default());
    registry.ready();
    let mailbox = Arc::new(Mailbox::default());
    let id = registry.connect(mailbox.clone(), StreamKind::Bidi)?;
    let topic = xmtp_proto::types::TopicKind::WelcomeMessagesV1
        .create([9; 32])
        .to_vec();
    registry.add(id, topic.clone(), 1)?;
    let hub = Arc::new(StreamHub {
        registry: registry.clone(),
        read: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://localhost/test")?,
        fetches: Arc::new(tokio::sync::Semaphore::new(super::super::FETCH_WORKERS)),
        maintenance: Arc::new(tokio::sync::Notify::new()),
        worker: tokio::spawn(pending()),
    });
    let (output, mut receiver) = mpsc::channel(OUTBOUND_FRAMES);
    let mut session = Session {
        static_subscription: false,
        hub,
        id,
        request_id: uuid::Uuid::new_v4(),
        config: config.clone(),
        mailbox: mailbox.clone(),
        output,
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

    let (_sent, handed) = tokio::sync::oneshot::channel();
    session.challenge = Some(Challenge {
        nonce: 1,
        handed,
        deadline: Some(Instant::now()),
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
    assert!(receiver.recv().await.is_none());
    assert_eq!(
        registry.add(id, topic, 2).unwrap_err().code(),
        tonic::Code::Unavailable
    );
}
