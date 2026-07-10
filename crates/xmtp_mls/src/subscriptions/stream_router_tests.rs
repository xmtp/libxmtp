//! Live integration tests for the XIP-83 [`StreamRouter`] over a real v3
//! backend (docker node). Native-only, v3-only — see the module gate.

use std::time::Duration;

use crate::context::XmtpSharedContext;
use crate::subscriptions::stream_router::{DEFAULT_STREAM_DEPTH, StreamRouter};
use crate::tester;
use crate::utils::MlsGroupExt;
use xmtp_api_d14n::{BidiConnection, BidiTransport, OpenError, V3Binding};

const WAIT: Duration = Duration::from_secs(20);

/// A transport whose opener dials the client's own v3 api client.
fn transport_for<C>(api: C) -> BidiTransport<V3Binding>
where
    C: xmtp_proto::api_client::XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    C::SubscribeStream: 'static,
{
    BidiTransport::new(move |initial| {
        let api = api.clone();
        async move {
            BidiConnection::open(&api, initial)
                .await
                .map_err(|e| Box::new(e) as OpenError)
        }
    })
}

/// Live delivery: a message sent after subscribing arrives decoded on the
/// router stream.
#[xmtp_common::test(unwrap_try = true)]
async fn router_delivers_live_messages() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;

    let transport = transport_for(bo.context.api().api_client.clone());
    let router = StreamRouter::new(bo.context.clone(), transport);
    let mut stream = router
        .stream_messages(vec![bo_group.group_id], DEFAULT_STREAM_DEPTH)
        .await?;

    group.send_msg(b"over the shared wire").await;

    let delivered = tokio::time::timeout(WAIT, stream.next())
        .await
        .expect("timed out waiting for a routed message")
        .expect("stream ended unexpectedly")?;
    assert_eq!(delivered.group_id, bo_group.group_id.to_vec());
    assert_eq!(delivered.decrypted_message_bytes, b"over the shared wire");
}

/// Catch-up: history sent while nobody streams is replayed from the durable
/// cursor when the stream opens (subscribe == cursored re-add).
#[xmtp_common::test(unwrap_try = true)]
async fn router_catches_up_from_durable_cursor() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?; // advances bo's durable cursor to "now"

    // History lands while bo is not streaming.
    group.send_msg(b"missed one").await;
    group.send_msg(b"missed two").await;

    let transport = transport_for(bo.context.api().api_client.clone());
    let router = StreamRouter::new(bo.context.clone(), transport);
    let mut stream = router
        .stream_messages(vec![bo_group.group_id], DEFAULT_STREAM_DEPTH)
        .await?;

    let mut caught_up = Vec::new();
    while caught_up.len() < 2 {
        let delivered = tokio::time::timeout(WAIT, stream.next())
            .await
            .expect("timed out waiting for catch-up replay")
            .expect("stream ended unexpectedly")?;
        caught_up.push(delivered.decrypted_message_bytes);
    }
    assert_eq!(
        caught_up,
        vec![b"missed one".to_vec(), b"missed two".to_vec()]
    );
}

/// Dropping the stream ends its lease, and a fresh stream on the same router
/// does NOT re-deliver what the first stream already delivered — the
/// streaming pipeline stores without advancing the durable cursor, so this
/// pins the stored-message (`messages_newer_than`) seeding.
#[xmtp_common::test(unwrap_try = true)]
async fn resubscribe_does_not_redeliver() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;

    let transport = transport_for(bo.context.api().api_client.clone());
    let router = StreamRouter::new(bo.context.clone(), transport);

    let mut stream = router
        .stream_messages(vec![bo_group.group_id], DEFAULT_STREAM_DEPTH)
        .await?;
    group.send_msg(b"delivered once").await;
    let first = tokio::time::timeout(WAIT, stream.next())
        .await
        .expect("timed out on first delivery")
        .expect("stream ended unexpectedly")?;
    assert_eq!(first.decrypted_message_bytes, b"delivered once");
    drop(stream);

    let mut stream = router
        .stream_messages(vec![bo_group.group_id], DEFAULT_STREAM_DEPTH)
        .await?;
    group.send_msg(b"after resubscribe").await;

    // The already-delivered message is stored above the durable cursor; the
    // server replays it, and the fresh stream must skip it.
    let delivered = tokio::time::timeout(WAIT, stream.next())
        .await
        .expect("timed out after resubscribe")
        .expect("stream ended unexpectedly")?;
    assert_eq!(
        delivered.decrypted_message_bytes, b"after resubscribe",
        "a re-subscribed stream must not re-deliver already-delivered history"
    );
}

/// Two conversation streams on one router each receive the same welcome —
/// per-stream dedup state means one stream's delivery never suppresses the
/// other's.
#[xmtp_common::test(unwrap_try = true)]
async fn sibling_conversation_streams_both_receive_a_welcome() {
    tester!(alix);
    tester!(bo);

    let transport = transport_for(bo.context.api().api_client.clone());
    let router = StreamRouter::new(bo.context.clone(), transport);
    let mut first = router
        .stream_conversations(None, false, None, DEFAULT_STREAM_DEPTH)
        .await?;
    let mut second = router
        .stream_conversations(None, false, None, DEFAULT_STREAM_DEPTH)
        .await?;

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;

    for (name, stream) in [("first", &mut first), ("second", &mut second)] {
        let conversation = tokio::time::timeout(WAIT, stream.next())
            .await
            .unwrap_or_else(|_| panic!("{name} stream timed out waiting for the welcome"))
            .expect("stream ended unexpectedly")?;
        assert_eq!(
            conversation.group_id, group.group_id,
            "{name} stream must surface the new conversation"
        );
    }
}
