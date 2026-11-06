//! Count bounds on repeated request fields, checked before tonic decodes.
//!
//! Prost builds every entry of a repeated field before a handler can count
//! the entries, and an empty entry is two bytes on the wire. On each gated
//! route this layer holds the first gRPC message of the body, walks its wire
//! format, and rejects a request with more entries than a bound before any
//! entry is decoded. It also rejects a body that carries a second message,
//! because tonic decodes every message of a unary body. Every other request
//! reaches tonic with the same bytes, trailers, and body error, so tonic keeps
//! the status it gives without the gate.

use crate::{api::publish_error::Reason, config::LimitsConfig, service::publish_invalid};
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{HeaderMap, Request, Response};
use http_body::Frame;
use http_body_util::{BodyExt, StreamBody};
use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
use std::{
    sync::Arc,
    task::{Context, Poll},
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
    counts: &'static [Count],
    reject: fn(&'static str) -> Status,
}

/// Each limit is the handler's own bound: the product of its per-item bounds,
/// or for `QueryNewest` the larger of its two topic limits, so the gate never
/// rejects a request those bounds admit. A repeated field added under one of
/// these messages needs its own count here.
// implements: API-304, PUSH-264
const ROUTES: &[GatedRoute] = &[
    GatedRoute {
        path: "/xmtp.backend.v1.PublishService/Publish",
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
        reject: invalid_argument,
        // SubscribeStaticRequest.topics
        counts: &[Count {
            path: &[&[1]],
            limit: |limits| limits.max_static_topics,
            message: "invalid static topic count",
        }],
    },
];

/// The rejection the read and lookup handlers give for too many entries.
const ITEM_LIMIT: &str = "request exceeds item limit";

/// The whole-request publish rejection: too large, with no envelope index.
fn publish_too_large(message: &'static str) -> Status {
    publish_invalid(None, Reason::TooLarge, message)
}

fn invalid_argument(message: &'static str) -> Status {
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

    /// A request on another route goes to tonic unread. On a gated route the
    /// gate holds the first message, checks every count of the route, and
    /// then reads to the end of the body; tonic gets the request only when no
    /// count is exceeded and no second message follows. A rejection goes back
    /// before tonic decodes any message, and the unread body is dropped.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let Some(route) = gated(&request) else {
            return Box::pin(self.inner.call(request));
        };
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
    if let Some(count) = route.counts.iter().find(|count| {
        exceeds(&message[PREFIX_BYTES..], count.path, (count.limit)(limits)) == Some(true)
    }) {
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
