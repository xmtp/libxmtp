//! Live delivery checks over the durable receiver and local reader.

use super::{
    Result, SubscribeError,
    local_delivery::{DeliveryScope, LocalDeliveryFilter},
    message_reader::MessageReader,
};
use crate::{context::XmtpSharedContext, tester, utils::MlsGroupExt};
use futures::StreamExt;
use std::time::Duration;
use xmtp_db::group_message::{GroupMessageKind, StoredGroupMessage};

const WAIT: Duration = Duration::from_secs(20);

#[cfg(not(target_arch = "wasm32"))]
mod tcp_proxy;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn the_same_reader_recovers_a_missed_commit_after_a_tcp_outage() {
    use super::incoming::{IncomingConnection, IncomingRegistration};
    use crate::utils::DefaultTestClientCreator;
    use xmtp_db::prelude::*;
    use xmtp_proto::api_client::{
        ApiBuilder, NetConnectConfig, XmtpMlsBidiStreams, XmtpTestClient,
    };

    let api = DefaultTestClientCreator::create().build()?;
    let address = api
        .host()
        .strip_prefix("http://")
        .expect("local HTTP test backend")
        .trim_end_matches('/');
    let proxy =
        tcp_proxy::TcpProxy::start(tokio::net::lookup_host(address).await?.collect()).await?;
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host(format!("http://{}", proxy.address).parse()?);
    let api = std::sync::Arc::new(xmtp_api_backend::TrackedStatsClient::new(
        xmtp_api_backend::BackendClient::new(builder.build()?),
    ));
    tester!(alix, disable_workers);
    tester!(bo, api_client: api, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let mut reader = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    group.send_msg(b"before outage").await;
    let before = next_application(&mut reader).await?;
    assert_eq!(before.decrypted_message_bytes, b"before outage");
    let control = reader.control();
    xmtp_common::wait_for_eq(
        || async {
            control
                .catch_up_snapshot()
                .topics
                .iter()
                .any(|topic| topic.registration == IncomingRegistration::Active)
        },
        true,
    )
    .await?;
    let generation = control.catch_up_snapshot().connection_generation;
    let refused = proxy.pause().await;
    xmtp_common::time::timeout(WAIT, proxy.wait_for_refusal_after(refused)).await?;
    assert_ne!(
        control.catch_up_snapshot().connection,
        IncomingConnection::Connected
    );
    group
        .update_group_name("committed during outage".into())
        .await?;
    group.send_msg(b"during outage").await;
    assert_ne!(bo_group.group_name()?, "committed during outage");
    proxy.resume().await;

    // No explicit receiver sync can hide a failed reader recovery.
    let missed = next_application(&mut reader).await?;
    assert_eq!(missed.decrypted_message_bytes, b"during outage");
    assert_ne!(missed.id, before.id);
    assert_eq!(bo_group.group_name()?, "committed during outage");
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
    xmtp_common::wait_for_eq(
        || async {
            let snapshot = control.catch_up_snapshot();
            snapshot.connection == IncomingConnection::Connected
                && snapshot.connection_generation > generation
        },
        true,
    )
    .await?;
    group.send_msg(b"after outage").await;
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"after outage"
    );
    bo_group.send_msg(b"reply after outage").await;
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"reply after outage"
    );
    group.receive().await?;
    let expected = [
        b"before outage".as_slice(),
        b"during outage",
        b"after outage",
        b"reply after outage",
    ];
    for peer in [&group, &bo_group] {
        let messages = peer
            .context
            .db()
            .get_group_messages(&peer.group_id, &Default::default())?;
        let actual: Vec<_> = messages
            .iter()
            .filter(|message| message.kind == GroupMessageKind::Application)
            .map(|message| message.decrypted_message_bytes.as_slice())
            .collect();
        assert_eq!(actual, expected);
    }
    reader.close();
}

async fn next_application<C: XmtpSharedContext + 'static>(
    reader: &mut MessageReader<C>,
) -> Result<StoredGroupMessage> {
    xmtp_common::time::timeout(WAIT, async {
        loop {
            let item = reader
                .next_delivery()
                .await?
                .ok_or(SubscribeError::GroupMessageNotFound)?;
            item.acknowledgement.check_owner()?;
            item.acknowledgement.acknowledge()?;
            if item.message.kind == GroupMessageKind::Application {
                return Ok(item.message);
            }
        }
    })
    .await
    .map_err(|_| SubscribeError::StreamStale)?
}

#[xmtp_common::test(unwrap_try = true)]
async fn durable_reader_delivers_live_messages() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let mut reader = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    group.send_msg(b"over the shared wire").await;
    let delivered = next_application(&mut reader).await?;
    assert_eq!(delivered.decrypted_message_bytes, b"over the shared wire");
    let envelope = alix
        .context
        .api()
        .query_latest_group_message(group.group_id)
        .await?
        .unwrap();
    assert!(envelope.envelope_hash.is_some());
    assert_eq!(delivered.envelope_hash, envelope.envelope_hash);
    assert_eq!(
        delivered.expiry_ns,
        envelope.expiry_ns.map(|expiry| expiry as i64)
    );
}

/// A group-only reader recovers a rejoin without a Welcome consumer.
#[rstest::rstest]
#[case::reader_before_removal(false)]
#[case::reader_opened_while_inactive(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn streamed_message_recovers_pending_rejoin_welcome(#[case] open_while_inactive: bool) {
    use super::incoming::{IncomingProcessing, IncomingRegistration};
    use xmtp_common::wait_for_eq;
    use xmtp_db::prelude::*;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None).unwrap();
    group.invite(&bo).await.unwrap();
    // Only the initial fixture join is explicit. Recovery has no Welcome consumer.
    bo.sync_welcomes().await.unwrap();
    let bo_group = bo.group(&group.group_id).unwrap();
    let open_reader = || {
        MessageReader::new(
            bo.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
        )
    };
    let mut reader = if open_while_inactive {
        None
    } else {
        Some(open_reader().unwrap())
    };
    group.send_msg(b"before removal").await;
    if let Some(reader) = &mut reader {
        assert_eq!(
            next_application(reader)
                .await
                .unwrap()
                .decrypted_message_bytes,
            b"before removal"
        );
    }
    group.remove_members(&[bo.inbox_id()]).await.unwrap();
    if open_while_inactive {
        bo_group.sync().await.unwrap();
        assert!(!bo_group.is_active().unwrap());
        // Start without the previous sync controller's in-memory retirement state.
        wait_for_eq(
            || async { bo.context.incoming_runtime().coordinator.lock().is_none() },
            true,
        )
        .await
        .unwrap();
        let mut opened = open_reader().unwrap();
        assert_eq!(
            next_application(&mut opened)
                .await
                .unwrap()
                .decrypted_message_bytes,
            b"before removal"
        );
        reader = Some(opened);
    }
    let mut reader = reader.unwrap();
    let control = reader.control();
    let topic = Topic::new_group_message(group.group_id);
    xmtp_common::time::timeout(WAIT, async {
        loop {
            if control.catch_up_snapshot().topics.iter().any(|entry| {
                entry.topic == topic && entry.registration == IncomingRegistration::Removed
            }) {
                break;
            }
            control.changed().await;
        }
    })
    .await
    .unwrap();
    let status = control.catch_up_snapshot();
    assert_eq!(status.topics.len(), 1);
    assert_eq!(status.topics[0].topic, topic);
    assert_eq!(status.processing, IncomingProcessing::Complete);
    assert!(!status.discovery_pending);

    group.send_msg(b"while removed").await;
    group.invite(&bo).await.unwrap();
    group.send_msg(b"after rejoin").await;
    assert_eq!(
        next_application(&mut reader)
            .await
            .unwrap()
            .decrypted_message_bytes,
        b"after rejoin"
    );
    assert!(bo_group.is_active().unwrap());
    let stored = bo
        .context
        .db()
        .get_group_messages(&group.group_id, &Default::default())
        .unwrap();
    let applications: Vec<_> = stored
        .into_iter()
        .filter(|message| message.kind == GroupMessageKind::Application)
        .map(|message| message.decrypted_message_bytes)
        .collect();
    assert_eq!(
        applications,
        vec![b"before removal".to_vec(), b"after rejoin".to_vec()]
    );
    assert_eq!(control.catch_up_snapshot().topics.len(), 1);
    reader.close();
}

#[xmtp_common::test(unwrap_try = true)]
async fn durable_reader_delivers_history_from_unary_sync() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    group.send_msg(b"missed one").await;
    group.send_msg(b"missed two").await;
    bo.group(&group.group_id)?.sync().await?;
    let mut reader = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"missed one"
    );
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"missed two"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn acknowledged_message_is_not_redelivered() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let scope = DeliveryScope::Groups(vec![group.group_id]);
    let mut reader = MessageReader::new(
        bo.context.clone(),
        scope.clone(),
        LocalDeliveryFilter::default(),
        None,
    )?;
    group.send_msg(b"delivered once").await;
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"delivered once"
    );
    drop(reader);
    let mut reader = MessageReader::new(
        bo.context.clone(),
        scope,
        LocalDeliveryFilter::default(),
        None,
    )?;
    group.send_msg(b"after resubscribe").await;
    assert_eq!(
        next_application(&mut reader).await?.decrypted_message_bytes,
        b"after resubscribe"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn sibling_conversation_streams_both_receive_a_welcome() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let mut first = Box::pin(bo.stream_conversations_owned(None, false).await?);
    let mut second = Box::pin(bo.stream_conversations_owned(None, false).await?);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    for stream in [&mut first, &mut second] {
        let conversation = xmtp_common::time::timeout(WAIT, stream.next())
            .await?
            .unwrap()?;
        assert_eq!(conversation.group_id, group.group_id);
    }
}
