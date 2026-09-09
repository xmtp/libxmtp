//! P3-TST-002: native stream limits use the Docker backend.
use super::*;
use futures::stream;
use xmtp_common::time::timeout;
use xmtp_proto::api_client::XmtpMlsBidiStreams;

/// P3-TST-002, P3-API-008: stream limit statuses surface after one RPC.
#[rstest]
#[case::structural_limit(tonic::Code::InvalidArgument)]
#[case::byte_limit(tonic::Code::OutOfRange)]
#[case::token_bucket(tonic::Code::ResourceExhausted)]
#[xmtp_common::test(unwrap_try = true)]
async fn limit_status_does_not_reopen_the_stream(#[case] code: tonic::Code) {
    use xmtp_proto::api::{BytesStream, mock::MockNetworkClient};
    let mut mock = MockNetworkClient::new();
    mock.expect_bidi_stream()
        .times(1)
        .return_once(move |_, _, _| {
            let started = wire::SubscribeResponse {
                response: Some(wire::subscribe_response::Response::Started(
                    Default::default(),
                )),
            };
            Ok(http::Response::new(BytesStream::new(stream::iter([
                Ok(started.encode_to_vec().into()),
                Err(status(code)),
            ]))))
        });
    let client = xmtp_api_backend::BackendClient::new(mock);
    let mut inbound = client
        .subscribe_bidi(stream::pending().boxed())
        .await
        .unwrap();
    assert!(matches!(
        inbound.next().await.unwrap().unwrap().response,
        Some(wire::subscribe_response::Response::Started(_))
    ));
    let error = inbound.next().await.unwrap().unwrap_err();
    assert_eq!(grpc_status(&error).unwrap().code(), code);
    assert!(inbound.next().await.is_none());
}

/// P3-TST-002: adds and removes accept the cap and reject one more entry.
#[rstest]
#[case::adds(true, BACKEND_DEFAULT_MAX_UPDATE_ADDS)]
#[case::removes(false, BACKEND_DEFAULT_MAX_UPDATE_REMOVES)]
#[xmtp_common::test(unwrap_try = true)]
async fn update_entry_boundary(#[case] adds: bool, #[case] cap: usize) {
    for count in [cap, cap + 1] {
        let api = backend();
        let topics: Vec<_> = welcome_topics(count)
            .into_iter()
            .map(|topic| wire::Topic {
                topic: topic.cloned_vec(),
            })
            .collect();
        let frame = wire::SubscribeRequest {
            request: Some(wire::subscribe_request::Request::Update(
                wire::subscribe_request::Update {
                    id: 1,
                    adds: if adds {
                        topics
                            .iter()
                            .cloned()
                            .map(|topic| wire::TopicQuery {
                                topic: Some(topic),
                                cursor: None,
                            })
                            .collect()
                    } else {
                        vec![]
                    },
                    removes: if adds { vec![] } else { topics.clone() },
                },
            )),
        };
        let sent = Arc::new(AtomicUsize::new(0));
        let counter = sent.clone();
        let (sender, requests) = futures::channel::mpsc::unbounded();
        sender.unbounded_send(frame).unwrap();
        let outbound = requests
            .inspect(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .boxed();
        let mut inbound = api
            .api_client
            .inner()
            .subscribe_bidi(outbound)
            .await
            .unwrap();
        let started = timeout(Duration::from_secs(30), inbound.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Some(wire::subscribe_response::Response::Started(started)) = started.response else {
            panic!("the backend must start each stream with its keepalive interval");
        };
        assert_eq!(
            u64::from(started.keepalive_interval_ms),
            BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS
        );
        let result = timeout(Duration::from_secs(30), inbound.next())
            .await
            .unwrap()
            .unwrap();
        if count == cap {
            let Some(wire::subscribe_response::Response::Applied(applied)) =
                result.unwrap().response
            else {
                panic!("a valid update must be acknowledged");
            };
            assert_eq!(applied.id, 1);
            assert_eq!(applied.added_targets.len(), if adds { cap } else { 0 });
            if adds {
                for (target, topic) in applied.added_targets.iter().zip(topics) {
                    assert_eq!(target.topic.as_ref(), Some(&topic));
                    assert_eq!(target.through_sequence_id, 0);
                }
            }
        } else {
            assert_eq!(
                grpc_status(&result.unwrap_err()).unwrap().code(),
                tonic::Code::InvalidArgument
            );
            assert!(inbound.next().await.is_none());
        }
        assert_eq!(sent.load(Ordering::SeqCst), 1);
        drop(sender);
    }
}

/// P3-TST-002: update and ping bursts accept the cap, then surface exhaustion.
#[rstest]
#[case::updates(
    true,
    BACKEND_DEFAULT_MAX_UPDATE_BURST,
    BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND
)]
#[case::pings(
    false,
    BACKEND_DEFAULT_MAX_PING_BURST,
    BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND
)]
#[xmtp_common::test(unwrap_try = true)]
async fn token_bucket_boundary(#[case] updates: bool, #[case] burst: u32, #[case] rate: u32) {
    for count in [burst, burst + 1] {
        let api = backend();
        let (sender, requests) = futures::channel::mpsc::unbounded();
        let mut inbound = api
            .api_client
            .inner()
            .subscribe_bidi(requests.boxed())
            .await
            .unwrap();
        assert!(matches!(
            timeout(Duration::from_secs(5), inbound.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .response,
            Some(wire::subscribe_response::Response::Started(_))
        ));
        let frame = |id: u32| wire::SubscribeRequest {
            request: Some(if updates {
                wire::subscribe_request::Request::Update(wire::subscribe_request::Update {
                    id: id.into(),
                    ..Default::default()
                })
            } else {
                wire::subscribe_request::Request::Ping(wire::Ping { nonce: id.into() })
            }),
        };
        let started = xmtp_common::time::Instant::now();
        const REPLY_BATCH: usize = 8;
        for first in (1..=burst).step_by(REPLY_BATCH) {
            let end = burst.min(first + REPLY_BATCH as u32 - 1);
            // Bound queued replies so output capacity does not mask the bucket.
            for id in first..=end {
                sender.unbounded_send(frame(id)).unwrap();
            }
            for id in first..=end {
                let response = timeout(Duration::from_secs(5), inbound.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                match response.response {
                    Some(wire::subscribe_response::Response::Applied(applied)) if updates => {
                        assert_eq!(applied.id, u64::from(id))
                    }
                    Some(wire::subscribe_response::Response::Pong(pong)) if !updates => {
                        assert_eq!(pong.nonce, u64::from(id))
                    }
                    other => panic!("unexpected burst response: {other:?}"),
                }
            }
        }
        let refill_interval = Duration::from_secs(1) / rate;
        if count > burst {
            assert!(
                started.elapsed() < refill_interval,
                "the burst must finish before a token can refill"
            );
            sender.unbounded_send(frame(count)).unwrap();
            let error = timeout(Duration::from_secs(5), inbound.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(
                grpc_status(&error).unwrap().code(),
                tonic::Code::ResourceExhausted
            );
            assert!(inbound.next().await.is_none());
        } else {
            xmtp_common::time::sleep(refill_interval).await;
            sender.unbounded_send(frame(burst + 1)).unwrap();
            let response = timeout(Duration::from_secs(5), inbound.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match response.response {
                Some(wire::subscribe_response::Response::Applied(applied)) if updates => {
                    assert_eq!(applied.id, u64::from(burst + 1))
                }
                Some(wire::subscribe_response::Response::Pong(pong)) if !updates => {
                    assert_eq!(pong.nonce, u64::from(burst + 1))
                }
                other => panic!("a refilled token must accept one more frame: {other:?}"),
            }
        }
    }
}

/// P3-TST-002, API-132: requests above the HTTP/2 cap queue until a slot opens.
#[rstest]
#[case(BACKEND_DEFAULT_MAX_HTTP2_STREAMS)]
#[xmtp_common::test(unwrap_try = true)]
async fn http2_stream_boundary(#[case] cap: usize) {
    let api = backend();
    let client = api.api_client.inner();
    let mut held = Vec::new();
    for _ in 0..cap {
        let (sender, requests) = futures::channel::mpsc::unbounded();
        let mut inbound = timeout(
            Duration::from_secs(5),
            client.subscribe_bidi(requests.boxed()),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(
            timeout(Duration::from_secs(5), inbound.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .response,
            Some(wire::subscribe_response::Response::Started(_))
        ));
        held.push((sender, inbound));
    }
    let (_sender, requests) = futures::channel::mpsc::unbounded();
    let waiting = client.subscribe_bidi(requests.boxed());
    futures::pin_mut!(waiting);
    assert!(
        timeout(Duration::from_millis(100), &mut waiting)
            .await
            .is_err(),
        "one extra stream must wait for a connection slot"
    );
    held.pop();
    let mut admitted = timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(5), admitted.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .response,
        Some(wire::subscribe_response::Response::Started(_))
    ));
}
