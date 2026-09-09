use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn a_resume_burst_during_an_outage_dials_once() {
    assert_resumes_keep_scheduled_backoff().await;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let servers: Servers = Arc::default();
    let dials = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(tokio::sync::Notify::new());
    let network_down = Arc::new(AtomicBool::new(true));
    let transport: BidiTransport<BackendBinding> = {
        let sink = servers.clone();
        let dials = dials.clone();
        let gate = gate.clone();
        let network_down = network_down.clone();
        BidiTransport::new(
            move |initial| {
                let n = dials.fetch_add(1, Ordering::SeqCst);
                let sink = sink.clone();
                let gate = gate.clone();
                let network_down = network_down.clone();
                async move {
                    if n == 1 {
                        gate.notified().await;
                        return Err(OpenError::retryable(std::io::Error::other("down")));
                    }
                    if n >= 2 && network_down.load(Ordering::SeqCst) {
                        return Err(OpenError::retryable(std::io::Error::other("down")));
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

    let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
    let mut server = take_server(&servers);
    let first = server.next_mutate().await;
    server.ack_empty(first.id);
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::CatchUpComplete)
    ));
    xmtp_common::time::timeout(WAIT, transport.suspend()).await??;

    let resumes: Vec<_> = (0..3)
        .map(|i| {
            let transport = transport.clone();
            tokio::spawn(async move {
                if i > 0 {
                    xmtp_common::time::sleep(Duration::from_millis(20)).await;
                }
                transport.resume().await
            })
        })
        .collect();
    xmtp_common::wait_for_ge(|| async { dials.load(Ordering::SeqCst) }, 2).await?;
    xmtp_common::time::sleep(Duration::from_millis(50)).await;
    network_down.store(false, Ordering::SeqCst);
    gate.notify_one();

    xmtp_common::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        dials.load(Ordering::SeqCst),
        2,
        "resumes deferred by the failed dial must park, not re-dial"
    );

    let mut server = wait_for_server(&servers).await;
    let resume = server.next_mutate().await;
    server.ack_empty(resume.id);
    for handle in resumes {
        xmtp_common::time::timeout(WAIT, handle).await?.unwrap()?;
    }
    assert_eq!(dials.load(Ordering::SeqCst), 3, "exactly one retry dial");

    server.send(messages(vec![group_msg(1, b"g1")], vec![]));
    assert!(matches!(
        recv(&mut alpha).await,
        Some(LeaseEvent::GroupMessages(_))
    ));
}

/// Resume notifications that arrive after a failed dial must wait for the
/// scheduled retry, just as notifications received during the dial do.
async fn assert_resumes_keep_scheduled_backoff() {
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
            Box::pin(async { Err(OpenError::retryable(std::io::Error::other("offline"))) })
        },
    );
    task.reconnect_delay = Duration::from_secs(5);
    task.arm_reconnect();
    let mut replies = Vec::new();
    for _ in 0..3 {
        let (reply, receiver) = oneshot::channel();
        replies.push(receiver);
        assert!(matches!(task.resume(reply).await, Flow::Continue));
    }
    assert_eq!(
        dials.load(Ordering::SeqCst),
        0,
        "resume must wait for the scheduled dial"
    );
    assert!(
        replies
            .iter_mut()
            .all(|reply| matches!(reply.try_recv(), Err(oneshot::error::TryRecvError::Empty)))
    );
}
