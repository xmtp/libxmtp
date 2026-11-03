use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_cancelled_waiter_does_not_cancel_leader() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let mut leader = Box::pin(query(&client));
    assert!(futures::poll!(&mut leader).is_pending());
    let mut waiter = Box::pin(query(&client));
    assert!(futures::poll!(&mut waiter).is_pending());
    drop(waiter);
    send.send(()).unwrap();
    leader.await?;
    assert_eq!(*script.calls.lock(), vec!["configuration", "query"]);
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_successful_refresh_discharges_pending_without_replacing_snapshot() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let refresh = client.refresh_server_configuration();
    let target = query(&client);
    futures::pin_mut!(refresh, target);
    assert!(futures::poll!(&mut refresh).is_pending());
    assert!(futures::poll!(&mut target).is_pending());
    send.send(()).unwrap();
    let (refresh, target) = futures::join!(refresh, target);
    refresh?;
    target?;
    assert_eq!(*script.calls.lock(), vec!["configuration", "query"]);
    client.close().await?;
}
