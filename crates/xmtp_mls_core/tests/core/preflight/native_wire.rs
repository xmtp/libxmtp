use super::*;
use xmtp_mls::context::CoreContext;
use xmtp_proto::api_client::XmtpMlsBidiStreams;

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_guards_direct_native_bidi_open() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let open = client
        .context
        .api()
        .api_client
        .subscribe_bidi(Box::pin(futures::stream::empty()));
    futures::pin_mut!(open);
    assert!(futures::poll!(&mut open).is_pending());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    send.send(()).unwrap();
    drop(open.await?);
    assert_eq!(*script.calls.lock(), vec!["configuration", "bidi"]);
    client.close().await?;
}
