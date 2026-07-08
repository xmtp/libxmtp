//! Live integration tests for the bidi callback adapters over a real v3
//! backend (docker node). Native-only, v3-only — see the module gate.

use std::sync::Arc;
use std::time::Duration;

use xmtp_common::StreamHandle;

use crate::Client;
use crate::tester;
use crate::utils::MlsGroupExt;

const WAIT: Duration = Duration::from_secs(20);

/// A message sent after subscribing arrives decoded through the callback.
#[xmtp_common::test(unwrap_try = true)]
async fn callback_stream_delivers_live_messages() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    group.send_msg(b"over the bidi pump").await;
    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the callback")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"over the bidi pump");
}

/// A new conversation surfaces on the conversations callback.
#[xmtp_common::test(unwrap_try = true)]
async fn callback_stream_surfaces_new_conversations() {
    tester!(alix);
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_conversations_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        false,
        move |conversation| {
            let _ = tx.send(conversation);
        },
        || {},
    );
    handle.wait_for_ready().await;

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;

    let conversation = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the conversation callback")
        .expect("callback channel closed")?;
    assert_eq!(conversation.group_id, group.group_id);
}

/// Two clients in one process ride the shared transport: each callback
/// stream still receives exactly its own client's traffic.
#[xmtp_common::test(unwrap_try = true)]
async fn sibling_clients_share_the_process_transport() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let bo_group = alix.create_group(None, None)?;
    bo_group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&bo_group.group_id)?.sync().await?;
    let caro_group = alix.create_group(None, None)?;
    caro_group.invite(&caro).await?;
    caro.sync_welcomes().await?;
    caro.group(&caro_group.group_id)?.sync().await?;

    let (bo_tx, mut bo_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut bo_handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = bo_tx.send(message);
        },
        || {},
    );
    let (caro_tx, mut caro_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut caro_handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(caro.client.clone()),
        None,
        None,
        move |message| {
            let _ = caro_tx.send(message);
        },
        || {},
    );
    bo_handle.wait_for_ready().await;
    caro_handle.wait_for_ready().await;

    bo_group.send_msg(b"for bo").await;
    caro_group.send_msg(b"for caro").await;

    let to_bo = tokio::time::timeout(WAIT, bo_rx.recv())
        .await
        .expect("timed out waiting for bo's callback")
        .expect("bo callback channel closed")?;
    assert_eq!(to_bo.decrypted_message_bytes, b"for bo");
    let to_caro = tokio::time::timeout(WAIT, caro_rx.recv())
        .await
        .expect("timed out waiting for caro's callback")
        .expect("caro callback channel closed")?;
    assert_eq!(to_caro.decrypted_message_bytes, b"for caro");
}
