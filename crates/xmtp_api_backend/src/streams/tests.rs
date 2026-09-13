#![allow(clippy::unwrap_used)]
use super::*;
use futures::{FutureExt, StreamExt};
use prost::Message;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use xmtp_common::RetryableError;
use xmtp_mls_validation::{
    parse_envelope,
    test_utils::{INSTALLATION_ID, inline_welcome_envelope},
};
use xmtp_proto::api::{BytesStream, mock::MockNetworkClient};

fn started(interval: u32) -> wire::SubscribeStaticResponse {
    wire::SubscribeStaticResponse {
        response: Some(wire::subscribe_static_response::Response::Started(
            wire::subscribe_static_response::Started {
                keepalive_interval_ms: interval,
                targets: vec![wire::CatchupTarget {
                    topic: Some(wire::Topic {
                        topic: Topic::new_welcome_message(INSTALLATION_ID.into()).cloned_vec(),
                    }),
                    through_sequence_id: 0,
                }],
            },
        )),
    }
}
fn keepalive() -> wire::SubscribeStaticResponse {
    wire::SubscribeStaticResponse {
        response: Some(wire::subscribe_static_response::Response::Keepalive(
            Default::default(),
        )),
    }
}
fn messages(sequence: u64) -> wire::SubscribeStaticResponse {
    messages_for(INSTALLATION_ID, sequence)
}
fn messages_for(installation: [u8; 32], sequence: u64) -> wire::SubscribeStaticResponse {
    let envelope = inline_welcome_envelope(installation);
    let parsed = parse_envelope(envelope.clone()).unwrap();
    wire::SubscribeStaticResponse {
        response: Some(wire::subscribe_static_response::Response::Messages(
            wire::subscribe_static_response::Messages {
                envelopes: vec![wire::ServerEnvelope {
                    envelope: Some(envelope),
                    meta: Some(wire::EnvelopeMeta {
                        cursor: Some(wire::Cursor {
                            sequence_id: sequence,
                        }),
                        server_ns: 123,
                        message_hash: Some(wire::MessageHash {
                            hash: Some(wire::message_hash::Hash::Sha256(
                                parsed.canonical.hash.to_vec(),
                            )),
                        }),
                        topic: Some(wire::Topic {
                            topic: parsed.topic.cloned_vec(),
                        }),
                        ..Default::default()
                    }),
                }],
            },
        )),
    }
}

fn started_for(
    request: &wire::SubscribeStaticRequest,
    interval: u32,
) -> wire::SubscribeStaticResponse {
    wire::SubscribeStaticResponse {
        response: Some(wire::subscribe_static_response::Response::Started(
            wire::subscribe_static_response::Started {
                keepalive_interval_ms: interval,
                targets: request
                    .topics
                    .iter()
                    .map(|query| wire::CatchupTarget {
                        topic: query.topic.clone(),
                        through_sequence_id: query.cursor.as_ref().unwrap().sequence_id,
                    })
                    .collect(),
            },
        )),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn id_only_subscription_starts_after_newest_cursor_without_a_gap() {
    let topic = Topic::new_welcome_message(INSTALLATION_ID.into());
    let expected = topic.clone();
    let mut mock = MockNetworkClient::new();
    mock.expect_request()
        .times(1)
        .returning(move |_, path, body| {
            assert_eq!(path.as_str(), "/xmtp.backend.v1.QueryService/QueryNewest");
            let request = wire::QueryNewestRequest::decode(body).unwrap();
            assert!(!request.include_full_envelope);
            assert_eq!(
                request.topics,
                vec![wire::Topic {
                    topic: expected.cloned_vec()
                }]
            );
            Ok(http::Response::new(
                wire::QueryNewestResponse {
                    results: vec![wire::query_newest_response::Result {
                        topic: Some(wire::Topic {
                            topic: expected.cloned_vec(),
                        }),
                        meta: Some(wire::EnvelopeMeta {
                            cursor: Some(wire::Cursor { sequence_id: 20 }),
                            ..Default::default()
                        }),
                        envelope: None,
                    }],
                }
                .encode_to_vec()
                .into(),
            ))
        });
    mock.expect_stream()
        .times(1)
        .returning(move |_, path, body| {
            assert_eq!(
                path.as_str(),
                "/xmtp.backend.v1.SubscriptionService/SubscribeStatic"
            );
            let request = wire::SubscribeStaticRequest::decode(body).unwrap();
            assert_eq!(request.topics.len(), 1);
            assert_eq!(request.topics[0].cursor.as_ref().unwrap().sequence_id, 20);
            assert_eq!(
                request.topics[0].topic.as_ref().unwrap().topic,
                topic.cloned_vec()
            );
            Ok(http::Response::new(BytesStream::new(
                stream::iter([started(0), keepalive(), messages(21)])
                    .map(|frame| Ok(frame.encode_to_vec().into())),
            )))
        });
    let client = BackendClient::new(mock);
    let installation: InstallationId = INSTALLATION_ID.into();
    let mut subscription = client.subscribe_welcome_messages(&[&installation]).await?;
    let welcome = subscription.next().await??;
    assert_eq!(welcome.sequence_id(), 21);
    assert_eq!(welcome.as_v1().unwrap().data, vec![0x10, 0x11]);
    assert!(subscription.next().await.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_subscriptions_split_at_the_topic_limit() {
    let mut cursors = TopicCursor::new();
    for index in 0..=BACKEND_DEFAULT_MAX_STATIC_TOPICS {
        let mut id = [0; 32];
        id[..8].copy_from_slice(&(index as u64).to_be_bytes());
        cursors.insert(Topic::new_welcome_message(id.into()), Cursor(index as u64));
    }
    let expected = cursors.clone();
    let total = Arc::new(AtomicUsize::new(0));
    let counted = total.clone();
    let mut mock = MockNetworkClient::new();
    mock.expect_stream().times(2).returning(move |_, _, body| {
        let request = wire::SubscribeStaticRequest::decode(body).unwrap();
        assert!(request.topics.len() <= BACKEND_DEFAULT_MAX_STATIC_TOPICS);
        counted.fetch_add(request.topics.len(), Ordering::SeqCst);
        for query in request.topics {
            let topic = Topic::parse(&query.topic.unwrap().topic).unwrap();
            assert_eq!(query.cursor.unwrap().sequence_id, expected[&topic].0);
        }
        Ok(http::Response::new(BytesStream::new(stream::pending())))
    });
    let client = BackendClient::new(mock);
    let mut subscription = client
        .subscribe_welcome_messages_with_cursors(&cursors)
        .await?;
    assert_eq!(total.load(Ordering::SeqCst), cursors.len());
    assert!(subscription.next().now_or_never().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn empty_subscription_does_not_open_a_wire_or_finish() {
    let client = BackendClient::new(MockNetworkClient::new());
    let mut subscription = client
        .subscribe_welcome_messages_with_cursors(&TopicCursor::new())
        .await?;
    assert!(subscription.next().now_or_never().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_stream_surfaces_bad_envelopes() {
    let mut mock = MockNetworkClient::new();
    mock.expect_stream().times(1).returning(|_, _, _| {
        let frame = wire::SubscribeStaticResponse {
            response: Some(wire::subscribe_static_response::Response::Messages(
                wire::subscribe_static_response::Messages {
                    envelopes: vec![Default::default()],
                },
            )),
        };
        Ok(http::Response::new(BytesStream::new(stream::iter([Ok(
            frame.encode_to_vec().into(),
        )]))))
    });
    let client = BackendClient::new(mock);
    let mut subscription = client
        .subscribe_welcome_messages_with_cursors(&TopicCursor::from([(
            Topic::new_welcome_message(INSTALLATION_ID.into()),
            Cursor(0),
        )]))
        .await?;
    let error = subscription.next().await?.unwrap_err();
    assert!(!error.is_retryable());
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn three_silent_intervals_end_the_stream_with_a_retryable_error() {
    let mut mock = MockNetworkClient::new();
    mock.expect_stream().times(1).returning(|_, _, _| {
        Ok(http::Response::new(BytesStream::new(
            stream::iter([Ok(started(1).encode_to_vec().into())]).chain(stream::pending()),
        )))
    });
    let client = BackendClient::new(mock);
    let mut subscription = client
        .subscribe_welcome_messages_with_cursors(&TopicCursor::from([(
            Topic::new_welcome_message(INSTALLATION_ID.into()),
            Cursor(0),
        )]))
        .await?;
    let error = timeout(Duration::from_secs(1), subscription.next())
        .await??
        .unwrap_err();
    assert!(matches!(error, ApiClientError::Expired(_)));
    assert!(error.is_retryable());
    assert!(subscription.next().await.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn silent_second_wire_ends_the_complete_subscription() {
    let cursors = (0..=BACKEND_DEFAULT_MAX_STATIC_TOPICS)
        .map(|index| {
            let mut id = [0; 32];
            id[..8].copy_from_slice(&(index as u64).to_be_bytes());
            (Topic::new_welcome_message(id.into()), Cursor(index as u64))
        })
        .collect();
    let mut calls = 0;
    let mut mock = MockNetworkClient::new();
    mock.expect_stream().times(2).returning(move |_, _, body| {
        calls += 1;
        let request = wire::SubscribeStaticRequest::decode(body).unwrap();
        let query = &request.topics[0];
        let topic = Topic::parse(&query.topic.as_ref().unwrap().topic).unwrap();
        let sequence = query.cursor.as_ref().unwrap().sequence_id + 1;
        let frames: BoxDynStream<'static, _> = if calls == 1 {
            Box::pin(
                stream::iter([
                    started_for(&request, 1000),
                    messages_for(topic.identifier().try_into().unwrap(), sequence),
                ])
                .chain(
                    xmtp_common::time::interval_stream(Duration::from_millis(1))
                        .map(|_| keepalive()),
                ),
            )
        } else {
            Box::pin(stream::iter([started_for(&request, 1)]).chain(stream::pending()))
        };
        Ok(http::Response::new(BytesStream::new(
            frames.map(|frame| Ok(frame.encode_to_vec().into())),
        )))
    });
    let client = BackendClient::new(mock);
    let mut subscription = client
        .subscribe_welcome_messages_with_cursors(&cursors)
        .await?;
    let message = timeout(Duration::from_secs(1), subscription.next()).await???;
    assert!(message.sequence_id() > 0);
    let error = timeout(Duration::from_secs(1), subscription.next())
        .await??
        .unwrap_err();
    assert!(matches!(error, ApiClientError::Expired(_)));
    assert!(error.is_retryable());
    assert!(subscription.next().now_or_never().unwrap().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_subscription_keeps_registration_targets_and_receipt_starts() {
    let topic = Topic::new_welcome_message(INSTALLATION_ID.into());
    let mut registration = started(0);
    let Some(wire::subscribe_static_response::Response::Started(started)) =
        &mut registration.response
    else {
        panic!("started frame");
    };
    started.targets[0].through_sequence_id = 40;
    let mut raw = normalize_static_stream(
        Box::pin(stream::iter(
            [registration, messages(21), messages(40)]
                .into_iter()
                .map(Ok),
        )),
        [(topic.clone(), Cursor(20))].into(),
        IncomingBatchLimits {
            max_rows: 10,
            max_bytes: 4096,
        },
    );
    let IncomingEvent::Registered { starts, targets } = raw.next().await?? else {
        panic!("registration");
    };
    assert_eq!(starts[&topic], Cursor(20));
    assert_eq!(targets[&topic], Cursor(40));
    let IncomingEvent::OrderedBatch(first) = raw.next().await?? else {
        panic!("first batch");
    };
    assert_eq!(first.after, Cursor(20));
    let IncomingEvent::OrderedBatch(second) = raw.next().await?? else {
        panic!("second batch");
    };
    assert_eq!(second.after, Cursor(21));
    assert!(matches!(raw.next().await??, IncomingEvent::Disconnected));
    assert!(raw.next().await.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn raw_subscription_rejects_missing_targets_and_messages_before_started() {
    let topic = Topic::new_welcome_message(INSTALLATION_ID.into());
    for frame in [
        wire::SubscribeStaticResponse {
            response: Some(wire::subscribe_static_response::Response::Started(
                Default::default(),
            )),
        },
        messages(1),
    ] {
        let mut raw = normalize_static_stream(
            Box::pin(stream::iter([Ok(frame)])),
            [(topic.clone(), Cursor(0))].into(),
            IncomingBatchLimits {
                max_rows: 10,
                max_bytes: 4096,
            },
        );
        assert!(!raw.next().await?.unwrap_err().is_retryable());
        assert!(raw.next().await.is_none());
    }
}
