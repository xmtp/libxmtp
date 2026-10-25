use super::*;
use crate::api;
use futures::{StreamExt, stream};
use prost::Message;
use std::{convert::Infallible, sync::Arc};
use tower::{ServiceExt, service_fn};
use xmtp_common::time::{Duration, timeout};
use xmtp_proto::xmtp::identity::associations::{IdentityAction, IdentityUpdate};

const PUBLISH: &str = "/xmtp.backend.v1.PublishService/Publish";
const ENVELOPES: &[&[u32]] = &[&[1]];
const ACTIONS: &[&[u32]] = &[&[1], &[4], &[1]];
const TOPICS: &[&[u32]] = &[&[3, 4]];
const KEYS: &[&[u32]] = &[&[3], &[3]];

/// An empty envelope entry of `PublishRequest`: field 1, length 0.
const EMPTY_ENTRY: [u8; 2] = [0x0a, 0x00];

fn identity_envelope(actions: usize) -> api::ClientEnvelope {
    api::ClientEnvelope {
        payload: Some(api::client_envelope::Payload::IdentityUpdate(
            IdentityUpdate {
                actions: vec![IdentityAction::default(); actions],
                client_timestamp_ns: 1,
                inbox_id: "inbox".into(),
            },
        )),
    }
}

fn subscription(keys: usize) -> api::Subscription {
    api::Subscription {
        topic: vec![1; 33],
        hmac_epoch_base: 1,
        hmac_keys: vec![vec![2; 42]; keys],
        include_commits: true,
    }
}

/// `count` empty envelope entries followed by `tail`.
fn entries(count: usize, tail: &[u8]) -> Vec<u8> {
    let mut message = EMPTY_ENTRY.repeat(count);
    message.extend_from_slice(tail);
    message
}

fn prefixed(message: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&(message.len() as u32).to_be_bytes());
    bytes.extend_from_slice(message);
    bytes
}

fn data(bytes: &[u8]) -> Result<Frame<Bytes>, Status> {
    Ok(Frame::data(Bytes::copy_from_slice(bytes)))
}

fn trailers() -> HeaderMap {
    let mut trailers = HeaderMap::new();
    trailers.insert("x-trailer", "kept".parse().unwrap());
    trailers
}

/// A body that yields `frames`, then ends, or stays open when `open`.
fn body(frames: Vec<Result<Frame<Bytes>, Status>>, open: bool) -> Body {
    let frames = stream::iter(frames);
    if open {
        Body::new(StreamBody::new(frames.chain(stream::pending()).boxed()))
    } else {
        Body::new(StreamBody::new(frames.boxed()))
    }
}

/// One step of a body as a service sees it.
#[derive(Debug, PartialEq)]
enum Seen {
    Data(Bytes),
    Trailers(HeaderMap),
    Error(tonic::Code, String),
}

async fn drain(mut body: Body) -> Vec<Seen> {
    let mut seen = Vec::new();
    while let Some(frame) = body.frame().await {
        seen.push(match frame {
            Ok(frame) => match frame.into_data() {
                Ok(data) => Seen::Data(data),
                Err(frame) => Seen::Trailers(frame.into_trailers().unwrap()),
            },
            Err(status) => Seen::Error(status.code(), status.message().to_owned()),
        });
    }
    seen
}

#[xmtp_common::test(unwrap_try = true)]
async fn exceeds_counts_complete_entries_at_the_path() {
    let request = api::PublishRequest {
        envelopes: vec![
            identity_envelope(2),
            api::ClientEnvelope::default(),
            identity_envelope(3),
        ],
    };
    let message = request.encode_to_vec();
    assert_eq!(exceeds(&message, ENVELOPES, 3), Some(false));
    assert_eq!(exceeds(&message, ENVELOPES, 2), Some(true));
    assert_eq!(exceeds(&message, ACTIONS, 5), Some(false));
    assert_eq!(exceeds(&message, ACTIONS, 4), Some(true));

    // A oneof field repeated in one envelope merges, so prost builds the
    // actions of both occurrences. The walk counts them all.
    let mut merged = identity_envelope(2).encode_to_vec();
    merged.extend(identity_envelope(2).encode_to_vec());
    let mut message = vec![0x0a];
    prost::encoding::encode_varint(merged.len() as u64, &mut message);
    message.extend(merged);
    let decoded = api::PublishRequest::decode(message.as_slice())?;
    assert!(matches!(
        &decoded.envelopes[..],
        [api::ClientEnvelope { payload: Some(api::client_envelope::Payload::IdentityUpdate(update)) }]
            if update.actions.len() == 4
    ));
    assert_eq!(exceeds(&message, ACTIONS, 4), Some(false));
    assert_eq!(exceeds(&message, ACTIONS, 3), Some(true));

    let request = api::UpdateSubscriptionsRequest {
        recipient_id: vec![1; 32],
        recipient_secret: vec![2; 32],
        adds: vec![subscription(3), subscription(2)],
        removes: vec![vec![3; 33]; 2],
    };
    let message = request.encode_to_vec();
    assert_eq!(exceeds(&message, TOPICS, 4), Some(false));
    assert_eq!(exceeds(&message, TOPICS, 3), Some(true));
    assert_eq!(exceeds(&message, KEYS, 5), Some(false));
    assert_eq!(exceeds(&message, KEYS, 4), Some(true));
}

#[xmtp_common::test(unwrap_try = true)]
async fn exceeds_ignores_unlisted_fields_and_other_depths() {
    // Adds and removes hold no keys, and the topic and epoch fields of an add
    // are not keys.
    let request = api::UpdateSubscriptionsRequest {
        recipient_id: vec![1; 32],
        recipient_secret: vec![2; 32],
        adds: vec![subscription(0); 4],
        removes: vec![vec![3; 33]; 4],
    };
    assert_eq!(exceeds(&request.encode_to_vec(), KEYS, 0), Some(false));
    // Actions of an identity update are not envelopes.
    let request = api::PublishRequest {
        envelopes: vec![identity_envelope(50)],
    };
    assert_eq!(exceeds(&request.encode_to_vec(), ENVELOPES, 1), Some(false));
    // Unknown varint, fixed64, length-delimited, and group fields are skipped
    // as prost skips them.
    let unknown = [
        0x48, 0x96, 0x01, // field 9, varint
        0x51, 1, 2, 3, 4, 5, 6, 7, 8, // field 10, fixed64
        0x5a, 0x02, 0x0a, 0x00, // field 11, length-delimited
        0x63, 0x08, 0x01, 0x64, // field 12, group holding a varint
    ];
    let message = [&EMPTY_ENTRY[..], &unknown, &EMPTY_ENTRY].concat();
    assert_eq!(
        api::PublishRequest::decode(message.as_slice())?
            .envelopes
            .len(),
        2
    );
    assert_eq!(exceeds(&message, ENVELOPES, 2), Some(false));
    assert_eq!(exceeds(&message, ENVELOPES, 1), Some(true));
}

#[xmtp_common::test(unwrap_try = true)]
async fn exceeds_stops_at_limit_plus_one() {
    // The tail is malformed, so prost fails, but only after building every
    // entry before it.
    for tail in [&[0xff][..], &[0x0a, 0x05]] {
        assert_eq!(exceeds(&entries(11, tail), ENVELOPES, 10), Some(true));
        assert_eq!(exceeds(&entries(10, tail), ENVELOPES, 10), None);
    }
    assert_eq!(exceeds(&entries(10, &[]), ENVELOPES, 10), Some(false));
}

#[xmtp_common::test(unwrap_try = true)]
async fn exceeds_is_none_on_malformed_wire_before_the_limit() {
    for (message, case) in [
        (entries(1, &[0x0a, 0x05, 0x00, 0x00]), "an incomplete entry"),
        (
            entries(1, &[0x08, 0x01]),
            "a listed field that is not length-delimited",
        ),
        (entries(1, &[0x80]), "a truncated key"),
        (entries(1, &[0x10]), "a varint field without its value"),
        (entries(1, &[0x0a, 0x80]), "a truncated length"),
    ] {
        assert!(
            api::PublishRequest::decode(message.as_slice()).is_err(),
            "{case}"
        );
        assert_eq!(exceeds(&message, ENVELOPES, 1), None, "{case}");
    }
    // Malformed wire inside an entry the walk enters.
    let mut envelope = identity_envelope(1).encode_to_vec();
    envelope.extend([0x22, 0x02, 0x0a, 0x05]);
    let mut message = vec![0x0a, envelope.len() as u8];
    message.extend(envelope);
    assert!(api::PublishRequest::decode(message.as_slice()).is_err());
    assert_eq!(exceeds(&message, ACTIONS, 1), None);
}

#[xmtp_common::test(unwrap_try = true)]
async fn first_message_forwards_a_prefix_tonic_rejects() {
    const MAX: usize = 64;
    let over = ((MAX + 1) as u32).to_be_bytes();
    for prefix in [
        [1, 0, 0, 0, 2],
        [2, 0, 0, 0, 2],
        [0, over[0], over[1], over[2], over[3]],
    ] {
        // The rest of the body never arrives, so a gate that reads past the
        // prefix never returns.
        let mut open = body(
            vec![
                data(&prefix[..2]),
                data(&[&prefix[2..], &[7; 9][..]].concat()),
            ],
            true,
        );
        let first = timeout(Duration::from_secs(5), first_message(&mut open, MAX)).await?;
        assert!(
            matches!(&first, First::Forward { bytes, end: End::Trailers(None) } if bytes[..] == prefix),
            "{prefix:?}"
        );
    }
    let at_limit = prefixed(&[7; MAX]);
    let mut open = body(vec![data(&at_limit)], true);
    let first = timeout(Duration::from_secs(5), first_message(&mut open, MAX)).await?;
    assert!(
        matches!(first, First::Message { bytes, overflow: false } if bytes[..] == at_limit[..])
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn first_message_never_holds_more_than_the_limit() {
    const MAX: usize = 8;
    let message = prefixed(&[7; MAX]);
    // The message ends inside a frame that carries a second message, and the
    // body stays open after it.
    let mut open = body(
        vec![
            data(&message[..2]),
            data(&[]),
            data(&message[2..9]),
            data(&[&message[9..], &prefixed(&[1; MAX])[..]].concat()),
        ],
        true,
    );
    let first = timeout(Duration::from_secs(5), first_message(&mut open, MAX)).await?;
    assert!(matches!(&first, First::Message { bytes, overflow: true } if bytes[..] == message[..]));
    // A message that fills its frame is returned without a further read.
    let mut open = body(vec![data(&message)], true);
    let first = timeout(Duration::from_secs(5), first_message(&mut open, MAX)).await?;
    assert!(
        matches!(&first, First::Message { bytes, overflow: false } if bytes[..] == message[..])
    );
    // A zero-length message is complete at its prefix.
    let mut open = body(vec![data(&[0, 0, 0, 0, 0, 0x0a])], true);
    let first = timeout(Duration::from_secs(5), first_message(&mut open, MAX)).await?;
    assert!(matches!(&first, First::Message { bytes, overflow: true } if bytes[..] == [0; 5]));
    // A body that ends inside the message is forwarded as it ended.
    for (end, expected) in [
        (None, Seen::Data(Bytes::copy_from_slice(&message[..7]))),
        (
            Some(Ok(Frame::trailers(trailers()))),
            Seen::Trailers(trailers()),
        ),
        (
            Some(Err(Status::cancelled("gone"))),
            Seen::Error(tonic::Code::Cancelled, "gone".into()),
        ),
    ] {
        let mut frames = vec![data(&message[..7])];
        frames.extend(end);
        let first = first_message(&mut body(frames, false), MAX).await;
        let First::Forward { bytes, end } = first else {
            panic!("an incomplete message is forwarded");
        };
        assert_eq!(bytes[..], message[..7]);
        assert_eq!(drain(replay(bytes, end)).await.pop()?, expected);
    }
    // The buffer grows with the bytes that arrive, so a stream that claims
    // the largest length and then stalls never holds that much.
    let mut held = Vec::new();
    for _ in 0..16 {
        append(&mut held, &[7], 1 << 20);
    }
    assert!(held.capacity() <= 32, "{}", held.capacity());
}

#[xmtp_common::test(unwrap_try = true)]
async fn rest_reports_extra_bytes() {
    // The body stays open after the extra byte, so a read to the end never
    // returns.
    let mut open = body(vec![data(&[]), data(&[0])], true);
    assert!(matches!(
        timeout(Duration::from_secs(5), rest(&mut open)).await?,
        Rest::Extra
    ));
    assert!(matches!(
        rest(&mut body(vec![data(&[]), Ok(Frame::trailers(trailers()))], false)).await,
        Rest::End(End::Trailers(Some(found))) if found == trailers()
    ));
    assert!(matches!(
        rest(&mut body(vec![data(&[]), Err(Status::cancelled("gone"))], false)).await,
        Rest::End(End::Error(status)) if status.code() == tonic::Code::Cancelled
    ));
    assert!(matches!(
        rest(&mut body(vec![data(&[])], false)).await,
        Rest::End(End::Trailers(None))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn encoded_request_is_passed_untouched() {
    let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let recorded = seen.clone();
    let gate = CountGateLayer {
        max_request_bytes: 1 << 20,
    }
    .layer(service_fn(move |request: Request<Body>| {
        let recorded = recorded.clone();
        async move {
            let seen = drain(request.into_body()).await;
            recorded.lock().push(seen);
            Ok::<_, Infallible>(Response::new(Body::empty()))
        }
    }));
    let call = |path: &str, encoding: Option<&str>, frames| {
        let mut request = Request::post(path);
        if let Some(encoding) = encoding {
            request = request.header("grpc-encoding", encoding);
        }
        gate.clone()
            .oneshot(request.body(body(frames, false)).unwrap())
    };
    let message = prefixed(
        &api::PublishRequest {
            envelopes: vec![identity_envelope(3); 2],
        }
        .encode_to_vec(),
    );
    let over = prefixed(&entries(MAX_PUBLISH_ENVELOPES + 1, &[]));
    let (head, tail) = message.split_at(9);
    let cases = [
        // Frames are joined, and trailers and a body error stay.
        (
            PUBLISH,
            None,
            vec![
                data(head),
                data(&[]),
                data(tail),
                Ok(Frame::trailers(trailers())),
            ],
            vec![
                Seen::Data(message.clone().into()),
                Seen::Trailers(trailers()),
            ],
        ),
        (
            PUBLISH,
            Some("identity"),
            vec![data(&message), Err(Status::data_loss("reset"))],
            vec![
                Seen::Data(message.clone().into()),
                Seen::Error(tonic::Code::DataLoss, "reset".into()),
            ],
        ),
        // An ungated route and a compressed body are not read.
        (
            "/xmtp.backend.v1.QueryService/Query",
            None,
            vec![data(&over), data(&over)],
            vec![
                Seen::Data(over.clone().into()),
                Seen::Data(over.clone().into()),
            ],
        ),
        (
            PUBLISH,
            Some("gzip"),
            vec![data(&over), data(&over)],
            vec![
                Seen::Data(over.clone().into()),
                Seen::Data(over.clone().into()),
            ],
        ),
    ];
    for (path, encoding, frames, expected) in cases {
        call(path, encoding, frames).await?;
        assert_eq!(seen.lock().pop()?, expected);
    }
    // An over-count request never reaches the service.
    let response = call(PUBLISH, None, vec![data(&over)]).await?;
    let status = Status::from_header_map(response.headers())?;
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert_eq!(status.message(), "publish request exceeds envelope limit");
    assert!(seen.lock().is_empty());
}
