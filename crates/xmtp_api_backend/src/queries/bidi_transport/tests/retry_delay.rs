//! A rate-limit end or refusal holds the next open until its delay passes.
use super::*;
use xmtp_api_grpc::error::{GrpcError, rate_limited_status};

const DELAY: Duration = Duration::from_secs(5);

fn rate_limited() -> ApiClientError {
    ApiClientError::client(GrpcError::Status(rate_limited_status("stream_rent", DELAY)))
}

/// A task with one lease and no wire, whose opener refuses the first dial
/// with a rate limit and counts every dial.
fn refusing_task() -> (
    LedgerTask<BackendBinding>,
    Arc<std::sync::atomic::AtomicUsize>,
    mpsc::UnboundedSender<Cmd<BackendBinding>>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _receiver) = mpsc::channel(1);
    ledger.register(&[(group_topic(b"g1"), 0)], events);
    let mut task = ledger_task(ledger, Outbox::default());
    let (commands, receiver) = mpsc::unbounded_channel();
    task.cmds = receiver;
    task.lease_cmds = commands.downgrade();
    let dials = Arc::new(AtomicUsize::new(0));
    let count = dials.clone();
    task.opener = Box::new(
        move |_| -> BoxDynFuture<'static, Result<Connection<BackendBinding>, OpenError>> {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(OpenError::new(rate_limited())) })
        },
    );
    (task, dials, commands)
}

/// The wire ends with a rate limit after a stable uptime. The next open waits
/// the delay, and the stable uptime does not reset the backoff.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn rate_limited_end_after_stable_uptime_waits_the_delay() {
    tokio::time::pause();
    let topic = group_topic(b"g1");
    let mut ledger = Ledger::<BackendBinding>::default();
    let (events, _receiver) = mpsc::channel(8);
    ledger.register(&[(topic.clone(), 0)], events);
    let (_, initial) = ledger.prepare_adds(vec![(topic, 0)]).remove(0);
    let (api, mut server) = mock_pair();
    let wire = BidiConnection::open(&api, initial).await?;
    let mut task = ledger_task(ledger, Outbox::default());
    let (_commands, receiver) = mpsc::unbounded_channel();
    task.cmds = receiver;
    task.conn = Some(wire);
    task.open_wire_span();
    server.next_mutate().await;
    tokio::time::advance(MIN_STABLE_UPTIME + Duration::from_secs(1)).await;

    server.to_client.send(Err(rate_limited()))?;
    while task.conn.as_mut()?.next().await.is_some() {}
    let ended = tokio::time::Instant::now();
    assert!(matches!(task.wire_died(), Flow::Continue));
    assert!(task.reconnect_delay > RECONNECT_INITIAL_DELAY);
    assert!(matches!(task.next_step().await, Step::Reconnect));
    assert!(tokio::time::Instant::now() >= ended + DELAY);
}

/// A reopen refused with a rate limit dials again only after the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn rate_limited_reopen_waits_the_delay_before_the_next_open() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    tokio::time::pause();
    let servers: Servers = Arc::default();
    let dials = Arc::new(AtomicUsize::new(0));
    let (dialed, mut dial_times) = mpsc::unbounded_channel();
    let transport: BidiTransport<BackendBinding> = {
        let sink = servers.clone();
        BidiTransport::new(
            move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let _ = dialed.send(tokio::time::Instant::now());
                let sink = sink.clone();
                async move {
                    if n == 1 {
                        return Err(OpenError::new(rate_limited()));
                    }
                    let (api, server) = mock_pair();
                    sink.lock().unwrap().push(server);
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            false,
        )
    };
    let mut lease = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        lease.next().await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    drop(server); // the wire dies; the reopen is refused

    let _opened = dial_times.recv().await?;
    let refused = dial_times.recv().await?;
    let reopened = dial_times.recv().await?;
    assert!(reopened - refused >= DELAY);
}

/// Resuming from suspension inside the delay schedules the open for its end.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn resume_inside_a_rate_limit_delay_waits_for_it() {
    use std::sync::atomic::Ordering;
    tokio::time::pause();
    let (mut task, dials, _commands) = refusing_task();
    assert!(matches!(task.reopen().await, AfterReopen::Proceed));
    let refused = tokio::time::Instant::now();
    assert_eq!(dials.load(Ordering::SeqCst), 1);

    let (reply, _suspended) = oneshot::channel();
    assert!(matches!(task.suspend(reply), Flow::Continue));
    let (reply, _resumed) = oneshot::channel();
    assert!(matches!(task.resume(reply).await, Flow::Continue));
    assert_eq!(
        dials.load(Ordering::SeqCst),
        1,
        "resume must not dial early"
    );
    assert!(task.reconnect_at >= refused + DELAY);
    assert!(
        task.reconnect_delay > RECONNECT_INITIAL_DELAY,
        "resume must not reset the backoff inside the delay"
    );
}

/// A first open refused with a rate limit holds the replacement lease's open.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn rate_limited_first_open_holds_the_next_open() {
    tokio::time::pause();
    let (mut task, dials, _commands) = refusing_task();
    task.ledger = Ledger::default();
    let refused = tokio::time::Instant::now();
    let (reply, denied) = oneshot::channel();
    task.lease(vec![(group_topic(b"g1"), 0)], 2, None, reply)
        .await;
    assert!(matches!(denied.await?, Err(TransportError::Open(_))));
    assert_eq!(dials.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(task.reconnect_at >= refused + DELAY);
}

/// A task whose live wire has just recorded a rate-limited end that the
/// ledger has not read yet.
async fn task_with_an_unread_rate_limited_end() -> (
    LedgerTask<BackendBinding>,
    LeaseId,
    Arc<std::sync::atomic::AtomicUsize>,
    mpsc::UnboundedSender<Cmd<BackendBinding>>,
) {
    let (mut task, dials, commands) = refusing_task();
    task.ledger = Ledger::default();
    let topic = group_topic(b"g2");
    let (events, _receiver) = mpsc::channel(8);
    let id = task.ledger.register(&[(topic.clone(), 0)], events);
    let (_, initial) = task.ledger.prepare_adds(vec![(topic, 0)]).remove(0);
    let (api, mut server) = mock_pair();
    task.conn = Some(BidiConnection::open(&api, initial).await.unwrap());
    task.open_wire_span();
    server.next_mutate().await;
    server.to_client.send(Err(rate_limited())).unwrap();
    xmtp_common::time::timeout(WAIT, async {
        while task.conn.as_ref().unwrap().failure().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    (task, id, dials, commands)
}

/// Dropping the last lease closes a wire whose rate-limited end was not read
/// yet. The next lease still waits for the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn an_idle_close_keeps_an_unread_rate_limit() {
    use std::sync::atomic::Ordering;
    let (mut task, id, dials, _commands) = task_with_an_unread_rate_limited_end().await;
    let closed = tokio::time::Instant::now();
    assert!(matches!(task.deref(id), Flow::Continue));
    assert!(task.conn.is_none());
    let (reply, _lease) = oneshot::channel();
    task.lease(vec![(group_topic(b"g3"), 0)], 2, None, reply)
        .await;
    assert_eq!(dials.load(Ordering::SeqCst), 0, "no open inside the delay");
    assert!(task.reconnect_at >= closed + DELAY);
}

/// Suspending closes a wire whose rate-limited end was not read yet. A
/// resume inside the delay still waits for it.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_suspend_close_keeps_an_unread_rate_limit() {
    use std::sync::atomic::Ordering;
    let (mut task, _id, dials, _commands) = task_with_an_unread_rate_limited_end().await;
    let (reply, _suspended) = oneshot::channel();
    assert!(matches!(task.suspend(reply), Flow::Continue));
    let (reply, _resumed) = oneshot::channel();
    assert!(matches!(task.resume(reply).await, Flow::Continue));
    assert_eq!(
        dials.load(Ordering::SeqCst),
        0,
        "resume must not dial early"
    );
}
