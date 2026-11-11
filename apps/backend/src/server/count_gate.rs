//! Count bounds on repeated request fields, checked before tonic decodes.
//!
//! Prost builds every entry of a repeated field before a handler can count
//! the entries, and an empty entry is two bytes on the wire. On each gated
//! unary route this layer holds the first gRPC message of the body, walks its
//! wire format, and rejects a request with more entries than a bound before
//! any entry is decoded. It also rejects a body that carries a second message,
//! because tonic decodes every message of a unary body. On a gated stream
//! route the handler starts at once, and the layer counts each message of the
//! body as it completes and ends the body with the rejection in place of the
//! first message over a bound. Every other request or message reaches tonic
//! with the same bytes, trailers, and body error, so tonic keeps the status it
//! gives without the gate.

use crate::{
    api::publish_error::Reason, config::LimitsConfig, service::publish_invalid, telemetry,
};
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{HeaderMap, Request, Response};
use http_body::Frame;
use http_body_util::{BodyExt, StreamBody};
use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use tonic::{Status, body::Body};
use tower::{Layer, Service};
use xmtp_configuration::{
    MAX_PUBLISH_ENVELOPES, MAX_PUBLISH_IDENTITY_ACTIONS, NOTIFICATION_BATCH_HMAC_KEYS,
    NOTIFICATION_BATCH_TOPICS,
};

#[cfg(test)]
mod tests;

/// The gRPC message prefix: a compression flag and a big-endian length.
const PREFIX_BYTES: usize = 5;
const MORE_THAN_ONE_MESSAGE: &str = "request carries more than one message";

/// A bound on the entries at one path of a request message.
struct Count {
    /// Field numbers at each depth. Entries of a field at the last depth are
    /// counted; entries of a field at an earlier depth are walked into.
    path: &'static [&'static [u32]],
    limit: fn(&LimitsConfig) -> usize,
    /// The rejection message. It names no topic, recipient, or index.
    message: &'static str,
}

struct GatedRoute {
    path: &'static str,
    /// Whether the client streams the request. A stream route's messages
    /// are counted one at a time while the handler runs.
    stream: bool,
    counts: &'static [Count],
    reject: fn(&'static str) -> Status,
}

/// Each limit is the handler's own bound: the product of its per-item bounds,
/// or for `QueryNewest` the larger of its two topic limits, so the gate never
/// rejects a request those bounds admit. A repeated field added under one of
/// these messages needs its own count here.
// implements: API-304, API-306, PUSH-264
const ROUTES: &[GatedRoute] = &[
    GatedRoute {
        path: "/xmtp.backend.v1.PublishService/Publish",
        stream: false,
        reject: publish_too_large,
        counts: &[
            // PublishRequest.envelopes
            Count {
                path: &[&[1]],
                limit: |_| MAX_PUBLISH_ENVELOPES,
                message: "publish request exceeds envelope limit",
            },
            // PublishRequest.envelopes, ClientEnvelope.identity_update,
            // IdentityUpdate.actions
            Count {
                path: &[&[1], &[4], &[1]],
                limit: |_| MAX_PUBLISH_IDENTITY_ACTIONS,
                message: "publish request exceeds identity action limit",
            },
        ],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.NotificationService/UpdateSubscriptions",
        stream: false,
        reject: invalid_argument,
        counts: &[
            // UpdateSubscriptionsRequest.adds and UpdateSubscriptionsRequest.removes
            Count {
                path: &[&[3, 4]],
                limit: |_| NOTIFICATION_BATCH_TOPICS,
                message: "request exceeds the subscription batch limit",
            },
            // UpdateSubscriptionsRequest.adds, Subscription.hmac_keys
            Count {
                path: &[&[3], &[3]],
                limit: |_| NOTIFICATION_BATCH_HMAC_KEYS,
                message: "request exceeds the subscription key limit",
            },
        ],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.QueryService/Query",
        stream: false,
        reject: invalid_argument,
        // QueryRequest.queries
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| limits.max_query_topics,
            message: ITEM_LIMIT,
        }],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.QueryService/QueryNewest",
        stream: false,
        reject: invalid_argument,
        // QueryNewestRequest.topics. The handler applies the smaller limit of
        // a full-envelope request after decoding.
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| {
                limits
                    .max_newest_metadata_topics
                    .max(limits.max_newest_full_topics)
            },
            message: ITEM_LIMIT,
        }],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.IdentityService/GetInboxIds",
        stream: false,
        reject: invalid_argument,
        // GetInboxIdsRequest.requests
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| limits.max_lookup_identifiers,
            message: ITEM_LIMIT,
        }],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures",
        stream: false,
        reject: invalid_argument,
        // VerifySmartContractWalletSignaturesRequest.signatures
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| limits.max_scw_signatures,
            message: ITEM_LIMIT,
        }],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        stream: false,
        reject: invalid_argument,
        // SubscribeStaticRequest.topics
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| limits.max_static_topics,
            message: "invalid static topic count",
        }],
    },
    GatedRoute {
        path: "/xmtp.backend.v1.SubscriptionService/Subscribe",
        stream: true,
        reject: update_invalid,
        counts: &[
            // SubscribeRequest.update, Update.adds
            Count {
                path: &[&[1], &[2]],
                limit: |limits| limits.max_update_adds,
                message: UPDATE_COUNT,
            },
            // SubscribeRequest.update, Update.removes
            Count {
                path: &[&[1], &[3]],
                limit: |limits| limits.max_update_removes,
                message: UPDATE_COUNT,
            },
        ],
    },
];

/// The rejection the read and lookup handlers give for too many entries.
const ITEM_LIMIT: &str = "request exceeds item limit";
/// The rejection the `Subscribe` handler gives for too many update entries.
const UPDATE_COUNT: &str = "invalid update id or item count";

/// The whole-request publish rejection: too large, with no envelope index.
fn publish_too_large(message: &'static str) -> Status {
    publish_invalid(None, Reason::TooLarge, message)
}

fn invalid_argument(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

/// The `Subscribe` rejection. The frame is an `Update` the session never
/// sees, so it is recorded here as the session records an invalid update.
fn update_invalid(message: &'static str) -> Status {
    telemetry::stream_frame_received(telemetry::Frame::Update);
    telemetry::stream_updated(telemetry::UpdateOutcome::Invalid);
    Status::invalid_argument(message)
}

/// Install innermost, inside gRPC-Web conversion, auth, and admission, so it
/// sees native frames of admitted requests only.
#[derive(Clone)]
pub(super) struct CountGateLayer {
    /// The configured count limits, and the decoding limit
    /// (`max_request_bytes`) tonic applies on every gated route.
    pub limits: Arc<LimitsConfig>,
}
impl<S> Layer<S> for CountGateLayer {
    type Service = CountGate<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CountGate {
            inner,
            limits: self.limits.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct CountGate<S> {
    inner: S,
    limits: Arc<LimitsConfig>,
}
impl<S> Service<Request<Body>> for CountGate<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// A request on another route goes to tonic unread. A request on a
    /// stream route goes to tonic at once, with its body read through
    /// [`Counted`]. On a gated unary route the gate holds the first message,
    /// checks every count of the route, and then reads to the end of the
    /// body; tonic gets the request only when no count is exceeded and no
    /// second message follows. A rejection goes back before tonic decodes any
    /// message, and the unread body is dropped.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let Some(route) = gated(&request) else {
            return Box::pin(self.inner.call(request));
        };
        if route.stream {
            let limits = self.limits.clone();
            let request = request.map(|body| Body::new(Counted::new(route, limits, body)));
            return Box::pin(self.inner.call(request));
        }
        // The readied service takes this request; the clone takes the next.
        let ready = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, ready);
        let limits = self.limits.clone();
        Box::pin(async move {
            let (parts, mut body) = request.into_parts();
            match admit(route, &mut body, &limits).await {
                Ok(body) => inner.call(Request::from_parts(parts, body)).await,
                Err(status) => Ok(status.into_http()),
            }
        })
    }
}

/// The route a request is gated on. A request whose `grpc-encoding` is not
/// `identity` is not gated: no gated service accepts compression, so tonic
/// refuses it before it reads the body.
fn gated(request: &Request<Body>) -> Option<&'static GatedRoute> {
    let identity = request
        .headers()
        .get("grpc-encoding")
        .is_none_or(|encoding| encoding.as_bytes() == b"identity");
    let path = request.uri().path();
    identity
        .then(|| ROUTES.iter().find(|route| route.path == path))
        .flatten()
}

/// Decide whether tonic may decode a gated body. Returns a body with the same
/// bytes and the same end, or the route's rejection.
async fn admit(route: &GatedRoute, body: &mut Body, limits: &LimitsConfig) -> Result<Body, Status> {
    let (message, overflow) = match first_message(body, limits.max_request_bytes).await {
        First::Forward { bytes, end } => return Ok(replay(bytes, end)),
        First::Message { bytes, overflow } => (bytes, overflow),
    };
    if let Some(count) = over(route, &message[PREFIX_BYTES..], limits) {
        return Err((route.reject)(count.message));
    }
    let rest = if overflow {
        Rest::Extra
    } else {
        rest(body).await
    };
    match rest {
        Rest::Extra => Err((route.reject)(MORE_THAN_ONE_MESSAGE)),
        Rest::End(end) => Ok(replay(message, end)),
    }
}

/// How a body ends after its data.
enum End {
    Trailers(Option<HeaderMap>),
    Error(Status),
}

/// The first message of a body, or what tonic gets when there is none.
enum First {
    /// The body ended or failed before a complete message, or its prefix is
    /// one tonic refuses. `bytes` is every data byte read, or that prefix.
    Forward { bytes: Bytes, end: End },
    /// A complete message with its prefix. `overflow` is true when the frame
    /// that completed it carried more data.
    Message { bytes: Bytes, overflow: bool },
}

/// What follows the first message.
enum Rest {
    End(End),
    /// A data byte: the start of a second message, which tonic would decode.
    Extra,
}

/// Read to the end of the first message, holding at most `PREFIX_BYTES +
/// max_request_bytes` bytes. A prefix whose flag is not 0 or whose length is
/// greater than `max_request_bytes` is forwarded alone, with no read after it:
/// tonic refuses it from those five bytes.
async fn first_message(body: &mut Body, max_request_bytes: usize) -> First {
    let mut held = Vec::new();
    let mut length = None;
    loop {
        let data = match body.frame().await {
            None => {
                return First::Forward {
                    bytes: held.into(),
                    end: End::Trailers(None),
                };
            }
            Some(Err(status)) => {
                return First::Forward {
                    bytes: held.into(),
                    end: End::Error(status),
                };
            }
            Some(Ok(frame)) => match frame.into_data() {
                Ok(data) => data,
                Err(frame) => {
                    return First::Forward {
                        bytes: held.into(),
                        end: End::Trailers(frame.into_trailers().ok()),
                    };
                }
            },
        };
        let mut data = data.as_ref();
        let need = match length {
            Some(length) => PREFIX_BYTES + length,
            None => {
                let take = (PREFIX_BYTES - held.len()).min(data.len());
                held.extend_from_slice(&data[..take]);
                data = &data[take..];
                let Some(&[flag, a, b, c, d]) = held.first_chunk::<PREFIX_BYTES>() else {
                    continue;
                };
                let len = u32::from_be_bytes([a, b, c, d]) as usize;
                if flag != 0 || len > max_request_bytes {
                    return First::Forward {
                        bytes: held.into(),
                        end: End::Trailers(None),
                    };
                }
                length = Some(len);
                PREFIX_BYTES + len
            }
        };
        let take = (need - held.len()).min(data.len());
        append(&mut held, &data[..take], need);
        if held.len() == need {
            return First::Message {
                bytes: held.into(),
                overflow: take < data.len(),
            };
        }
    }
}

/// Grow `held` by doubling, but never past `need`, so a claimed length is
/// never allocated before its bytes arrive. The caller keeps the result
/// within `need`.
fn append(held: &mut Vec<u8>, data: &[u8], need: usize) {
    let len = held.len() + data.len();
    if len > held.capacity() {
        let target = len.max(2 * held.capacity()).min(need);
        held.reserve_exact(target - held.len());
    }
    held.extend_from_slice(data);
}

/// Read the body after its first message. Empty data frames carry nothing
/// and are skipped.
async fn rest(body: &mut Body) -> Rest {
    loop {
        match body.frame().await {
            None => return Rest::End(End::Trailers(None)),
            Some(Err(status)) => return Rest::End(End::Error(status)),
            Some(Ok(frame)) => match frame.into_data() {
                Ok(data) if data.is_empty() => {}
                Ok(_) => return Rest::Extra,
                Err(frame) => return Rest::End(End::Trailers(frame.into_trailers().ok())),
            },
        }
    }
}

/// A body that yields `bytes` and then ends as the original body did.
fn replay(bytes: Bytes, end: End) -> Body {
    let data = (!bytes.is_empty()).then(|| Ok(Frame::data(bytes)));
    let end = match end {
        End::Trailers(trailers) => trailers.map(|trailers| Ok(Frame::trailers(trailers))),
        End::Error(status) => Some(Err(status)),
    };
    Body::new(StreamBody::new(futures::stream::iter(
        data.into_iter().chain(end),
    )))
}

/// The first count of `route` that `message`, without its prefix, exceeds.
fn over<'a>(route: &'a GatedRoute, message: &[u8], limits: &LimitsConfig) -> Option<&'a Count> {
    route
        .counts
        .iter()
        .find(|count| exceeds(message, count.path, (count.limit)(limits)) == Some(true))
}

/// The body of a stream route as tonic reads it. Each message is held until
/// it is complete, holding at most `PREFIX_BYTES + max_request_bytes` bytes,
/// and is then counted before tonic gets it. A message over a count of the
/// route is never forwarded: the body ends with the route's rejection in its
/// place, which tonic hands the handler as the stream's error. A prefix tonic
/// refuses is forwarded alone and the body then ends, as on a unary route,
/// so tonic decodes no message after it even when it is polled again. Every
/// message before these, and the bytes, trailers, or body error the body
/// ends with, reach tonic unchanged.
// implements: API-306
struct Counted {
    body: Body,
    route: &'static GatedRoute,
    limits: Arc<LimitsConfig>,
    /// The bytes of the current message read so far, prefix included.
    held: Vec<u8>,
    /// The length of the current message, once its prefix is held.
    length: Option<usize>,
    /// Frames to give tonic before the body is read again.
    ready: VecDeque<Result<Frame<Bytes>, Status>>,
    /// The body is read no further: it ended, failed, or had trailers, or a
    /// message was over a count or had a prefix tonic refuses.
    ended: bool,
}

impl Counted {
    fn new(route: &'static GatedRoute, limits: Arc<LimitsConfig>, body: Body) -> Self {
        Self {
            body,
            route,
            limits,
            held: Vec::new(),
            length: None,
            ready: VecDeque::new(),
            ended: false,
        }
    }

    /// Queue each message that `data` completes, or the rejection in place
    /// of the first one over a count, and hold the rest.
    fn count(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let need = match self.length {
                Some(length) => PREFIX_BYTES + length,
                None => {
                    let take = (PREFIX_BYTES - self.held.len()).min(data.len());
                    self.held.extend_from_slice(&data[..take]);
                    data = &data[take..];
                    let Some(&[flag, a, b, c, d]) = self.held.first_chunk::<PREFIX_BYTES>() else {
                        return;
                    };
                    let length = u32::from_be_bytes([a, b, c, d]) as usize;
                    // Tonic refuses this message from its prefix, as in
                    // `first_message`.
                    if flag != 0 || length > self.limits.max_request_bytes {
                        self.finish(None);
                        return;
                    }
                    self.length = Some(length);
                    PREFIX_BYTES + length
                }
            };
            let take = (need - self.held.len()).min(data.len());
            append(&mut self.held, &data[..take], need);
            data = &data[take..];
            if self.held.len() < need {
                return;
            }
            self.length = None;
            let message = std::mem::take(&mut self.held);
            if let Some(count) = over(self.route, &message[PREFIX_BYTES..], &self.limits) {
                self.ready
                    .push_back(Err((self.route.reject)(count.message)));
                self.ended = true;
                return;
            }
            self.ready.push_back(Ok(Frame::data(message.into())));
        }
    }

    /// Queue the held bytes and then `end`, and read the body no further.
    fn finish(&mut self, end: Option<Result<Frame<Bytes>, Status>>) {
        if !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            self.ready.push_back(Ok(Frame::data(held.into())));
        }
        self.ready.extend(end);
        self.ended = true;
    }
}

impl http_body::Body for Counted {
    type Data = Bytes;
    type Error = Status;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        let this = &mut *self;
        loop {
            if let Some(frame) = this.ready.pop_front() {
                return Poll::Ready(Some(frame));
            }
            if this.ended {
                return Poll::Ready(None);
            }
            match ready!(Pin::new(&mut this.body).poll_frame(cx)) {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) => this.count(&data),
                    Err(frame) => this.finish(Some(Ok(frame))),
                },
                Some(Err(status)) => this.finish(Some(Err(status))),
                None => this.finish(None),
            }
        }
    }
}

/// Whether `message` holds more than `limit` entries at `path`, summed over
/// the whole message: every occurrence of a listed field counts, so entries a
/// oneof or a repeated singular message would merge count as well.
///
/// The walk stops at `limit + 1` entries and returns `Some(true)`. It returns
/// `None` when the wire is malformed before that point, including a listed
/// field that is not length-delimited or not complete. Prost then fails the
/// decode at that point, having built at most `limit` entries.
fn exceeds(message: &[u8], path: &[&[u32]], limit: usize) -> Option<bool> {
    let mut count = 0;
    match walk(message, path, limit, &mut count) {
        Ok(()) => Some(false),
        Err(Stop::Exceeded) => Some(true),
        Err(Stop::Malformed) => None,
    }
}

enum Stop {
    Exceeded,
    Malformed,
}

fn walk(mut buffer: &[u8], path: &[&[u32]], limit: usize, count: &mut usize) -> Result<(), Stop> {
    let Some((fields, deeper)) = path.split_first() else {
        return Ok(());
    };
    while !buffer.is_empty() {
        let (field, wire_type) = decode_key(&mut buffer).map_err(|_| Stop::Malformed)?;
        if !fields.contains(&field) {
            skip_field(wire_type, field, &mut buffer, DecodeContext::default())
                .map_err(|_| Stop::Malformed)?;
            continue;
        }
        if wire_type != WireType::LengthDelimited {
            return Err(Stop::Malformed);
        }
        let length = decode_varint(&mut buffer).map_err(|_| Stop::Malformed)?;
        let length = usize::try_from(length)
            .ok()
            .filter(|length| *length <= buffer.len())
            .ok_or(Stop::Malformed)?;
        let (entry, after) = buffer.split_at(length);
        buffer = after;
        if deeper.is_empty() {
            *count += 1;
            if *count > limit {
                return Err(Stop::Exceeded);
            }
        } else {
            walk(entry, deeper, limit, count)?;
        }
    }
    Ok(())
}
