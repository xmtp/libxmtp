//! d14n (xmtpv4 `QueryApi`) binding for the XIP-83 bidirectional subscription
//! connection.
//!
//! The control core lives in [`crate::queries::bidi`]; this supplies the d14n
//! wire vocabulary (`xmtpv4.message_api` frames over the `QueryApi/Subscribe`
//! RPC) and turns each delivered `OriginatorEnvelope` batch into unified
//! `xmtp_proto::types::{GroupMessage, WelcomeMessage}` with one fallible
//! extraction pass per envelope. Native-only.
//!
//! It deliberately does **not** reorder. Per XIP-49 §1.3 the broadcast network is
//! not required to totally order, and the payloads that do need strict ordering
//! (commits, identity updates) are chain-anchored to a single fixed originator
//! (`Originators::MLS_COMMITS`), so MLS's epoch chain is the real dependency and
//! gates processing natively — an out-of-order application message just waits for
//! its epoch. XIP-49 §3.3.5 makes cross-originator causal ordering explicitly
//! optional ("clients are free to define an ordering strategy... for additional
//! trustlessness... at the cost of additional complexity"), so it belongs — if a
//! consumer wants it — as a durable *client-level* icebox, not bolted onto a
//! single per-process transport. This binding just extracts and fans out, exactly
//! like the v3 binding hands its messages straight through.

use crate::protocol::Envelope;
use crate::queries::bidi::{BidiBinding, Connection, Event, Inbound, parse_topics};
use futures::StreamExt;
use futures::stream::BoxStream;
use prost::Message;
use prost::bytes::Bytes;
use xmtp_proto::ApiEndpoint;
use xmtp_proto::api::{ApiClientError, Client, XmtpStream};
use xmtp_proto::types::{GroupMessage, WelcomeMessage};
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    Ping, Pong, SubscribeRequest, SubscribeResponse, subscribe_request, subscribe_response,
};

use super::D14nClient;

const SUBSCRIBE_PATH: &str = "/xmtp.xmtpv4.message_api.QueryApi/Subscribe";

/// The d14n (xmtpv4 `QueryApi`) wire binding for a bidi subscription. Stateless:
/// it extracts each delivered `OriginatorEnvelope` batch into unified messages,
/// per envelope, and surfaces them in wire order. No cross-originator reordering
/// happens here — see the module docs for why (XIP-49 makes it optional and MLS
/// epoch-gates natively). Symmetric with the v3 binding, which likewise passes
/// its messages straight through.
pub struct D14nBinding;

/// A d14n bidirectional subscription connection (XIP-83). See [`Connection`].
pub type D14nBidiConnection = Connection<D14nBinding>;
/// Events surfaced by a d14n [`D14nBidiConnection`], in wire order. Messages are
/// the unified `xmtp_proto::types` shapes, not raw protobuf.
pub type D14nBidiEvent = Event<GroupMessage, WelcomeMessage>;

fn request_frame(request: subscribe_request::v1::Request) -> SubscribeRequest {
    SubscribeRequest {
        version: Some(subscribe_request::Version::V1(subscribe_request::V1 {
            request: Some(request),
        })),
    }
}

/// Open the d14n bidi `Subscribe` transport: encode outbound frames and hand the
/// stream to the inner transport `Client`, returning the decoded inbound stream.
// Spans the open handshake (not the stream's lifetime) as `rpc.subscribe_bidi`,
// mirroring the v3 transport — this free fn is the d14n RPC boundary, the same
// layer the other `rpc_span`s live at for unary calls.
#[xmtp_common::rpc_span]
async fn subscribe_bidi<C: Client>(
    client: &C,
    requests: BoxStream<'static, SubscribeRequest>,
) -> Result<XmtpStream<SubscribeResponse>, ApiClientError> {
    tracing::debug!("opening d14n bidirectional subscription");
    let outbound = requests.map(|frame| Bytes::from(frame.encode_to_vec()));
    let response = client
        .bidi_stream(
            http::Request::builder(),
            http::uri::PathAndQuery::from_static(SUBSCRIBE_PATH),
            Box::pin(outbound),
        )
        .await
        .map_err(|e| e.endpoint(SUBSCRIBE_PATH.to_string()))?;
    Ok(XmtpStream::new(
        response.into_body(),
        ApiEndpoint::Path(SUBSCRIBE_PATH.to_string()),
    ))
}

impl BidiBinding for D14nBinding {
    type Request = SubscribeRequest;
    type Response = SubscribeResponse;
    type Mutate = subscribe_request::v1::Mutate;
    type GroupMessage = GroupMessage;
    type WelcomeMessage = WelcomeMessage;

    fn mutate_frame(mutate: subscribe_request::v1::Mutate) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Mutate(mutate))
    }

    fn ping_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Ping(Ping { nonce }))
    }

    fn pong_frame(nonce: u64) -> SubscribeRequest {
        request_frame(subscribe_request::v1::Request::Pong(Pong { nonce }))
    }

    fn handle(response: SubscribeResponse) -> Inbound<GroupMessage, WelcomeMessage> {
        let Some(subscribe_response::Version::V1(v1)) = response.version else {
            // A version we did not speak; XIP-83 pins responses to the request
            // version, so this is a server bug — skip, don't die.
            tracing::warn!("d14n bidi subscription received unknown response version");
            return Inbound::Skip;
        };
        use subscribe_response::v1::Response;
        match v1.response {
            // Liveness is internal; the core auto-pongs and correlates probes.
            Some(Response::Ping(ping)) => Inbound::Ping(ping.nonce),
            Some(Response::Pong(pong)) => Inbound::Pong(pong.nonce),
            Some(Response::Started(started)) => Inbound::Emit(Event::Started {
                keepalive_interval_ms: started.keepalive_interval_ms,
                capabilities: started.capabilities,
            }),
            Some(Response::CatchupComplete(complete)) => Inbound::Emit(Event::CatchUpComplete {
                mutate_id: complete.mutate_id,
            }),
            Some(Response::TopicsLive(live)) => Inbound::Emit(Event::TopicsLive {
                topics: parse_topics(live.topics),
            }),
            // Delivery: extract the envelopes into unified messages.
            Some(Response::Envelopes(envelopes)) => extract(envelopes.envelopes),
            // A future-revision arm: informational frames are safe to skip.
            None => Inbound::Skip,
        }
    }
}

/// Turn a delivered `OriginatorEnvelope` batch into unified messages, surfaced in
/// wire order (no reordering — see the module docs). One fallible extraction pass
/// per envelope via [`Envelope::group_message`]/[`Envelope::welcome_message`], so
/// a bad envelope — unreadable nesting, or a payload its extractor rejects — is
/// logged and skipped *alone*. It must never sink the valid messages delivered
/// alongside it: the consumer's cursors advance past a dropped batch, so anything
/// discarded here is never re-fetched.
fn extract(envelopes: Vec<OriginatorEnvelope>) -> Inbound<GroupMessage, WelcomeMessage> {
    let mut group = Vec::new();
    let mut welcome = Vec::new();
    for envelope in envelopes {
        match envelope.group_message() {
            Ok(Some(message)) => {
                group.push(message);
                continue;
            }
            // Not a group payload — try the welcome extractor below.
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("d14n bidi: skipping undecodable envelope: {e}");
                continue;
            }
        }
        match envelope.welcome_message() {
            Ok(Some(message)) => welcome.push(message),
            // Identity-update / key-package delivery is not implemented yet
            // (adopted later via XIP-83 `Started` capabilities); warn so a
            // subscription to such a topic fails loudly instead of looking
            // healthy while its data is dropped.
            Ok(None) => tracing::warn!(
                "d14n bidi: dropping delivered envelope of an unhandled payload kind"
            ),
            Err(e) => tracing::warn!("d14n bidi: skipping undecodable welcome envelope: {e}"),
        }
    }
    Inbound::Messages { group, welcome }
}

impl D14nBidiConnection {
    /// Open the d14n stream and send `initial` as the first Mutate (it names the
    /// initial topic set with per-topic resume *vector* cursors; XIP-83 client
    /// req 3). Uses only the client's transport; the binding is stateless.
    pub async fn open<C, S>(
        client: &D14nClient<C, S>,
        initial: subscribe_request::v1::Mutate,
    ) -> Result<Self, ApiClientError>
    where
        C: Client,
    {
        Self::start(initial, |outbound| subscribe_bidi(&client.client, outbound)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::extractors::test_utils::TestEnvelopeBuilder;
    use xmtp_proto::types::{Topic, TopicKind};

    fn response(r: subscribe_response::v1::Response) -> SubscribeResponse {
        SubscribeResponse {
            version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                response: Some(r),
            })),
        }
    }

    fn wire_topic(kind: TopicKind, identifier: &[u8]) -> Vec<u8> {
        // The production constructor, so these tests can't drift from the real
        // kind-prefixed wire layout.
        kind.create(identifier).into()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn classifies_control_frames() {
        let started = D14nBinding::handle(response(subscribe_response::v1::Response::Started(
            subscribe_response::v1::Started {
                keepalive_interval_ms: 30_000,
                capabilities: vec![7],
            },
        )));
        assert!(matches!(
            started,
            Inbound::Emit(Event::Started {
                keepalive_interval_ms: 30_000,
                ..
            })
        ));

        let ping = D14nBinding::handle(response(subscribe_response::v1::Response::Ping(Ping {
            nonce: 9,
        })));
        assert!(matches!(ping, Inbound::Ping(9)));

        let pong = D14nBinding::handle(response(subscribe_response::v1::Response::Pong(Pong {
            nonce: 9,
        })));
        assert!(matches!(pong, Inbound::Pong(9)));

        let complete =
            D14nBinding::handle(response(subscribe_response::v1::Response::CatchupComplete(
                subscribe_response::v1::CatchupComplete { mutate_id: 42 },
            )));
        assert!(matches!(
            complete,
            Inbound::Emit(Event::CatchUpComplete { mutate_id: 42 })
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn parses_topics_live_and_skips_malformed() {
        let good = wire_topic(TopicKind::GroupMessagesV1, b"group");
        let bad = vec![0xFF, 1, 2, 3]; // unknown kind byte
        let live = D14nBinding::handle(response(subscribe_response::v1::Response::TopicsLive(
            subscribe_response::v1::TopicsLive {
                topics: vec![good.clone(), bad],
            },
        )));
        let Inbound::Emit(Event::TopicsLive { topics }) = live else {
            panic!("expected TopicsLive");
        };
        // The malformed topic is dropped; the valid one survives.
        assert_eq!(topics, vec![Topic::try_from(good).unwrap()]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn skips_unknown_response_version() {
        assert!(matches!(
            D14nBinding::handle(SubscribeResponse { version: None }),
            Inbound::Skip
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn empty_envelope_batch_yields_empty_messages() {
        let out = D14nBinding::handle(response(subscribe_response::v1::Response::Envelopes(
            subscribe_response::v1::Envelopes { envelopes: vec![] },
        )));
        match out {
            Inbound::Messages { group, welcome } => {
                assert!(group.is_empty());
                assert!(welcome.is_empty());
            }
            _ => panic!("expected Messages"),
        }
    }

    /// One envelope that fails its *extractor* (nesting decodes, payload is
    /// rejected — here a 3-byte installation key that can't be an
    /// `InstallationId`) must be skipped alone: the valid welcome delivered in
    /// the same frame survives. A batch-level short-circuit would drop both.
    #[xmtp_common::test(unwrap_try = true)]
    fn bad_payload_is_skipped_without_dropping_the_batch() {
        let bad = TestEnvelopeBuilder::new()
            .with_welcome_message(vec![1, 2, 3])
            .build();
        let good = TestEnvelopeBuilder::new()
            .with_welcome_message(vec![7u8; 32])
            .build();
        let out = D14nBinding::handle(response(subscribe_response::v1::Response::Envelopes(
            subscribe_response::v1::Envelopes {
                envelopes: vec![bad, good],
            },
        )));
        match out {
            Inbound::Messages { group, welcome } => {
                assert!(group.is_empty());
                assert_eq!(
                    welcome.len(),
                    1,
                    "the valid welcome must survive its bad neighbor"
                );
            }
            _ => panic!("expected Messages, not a dropped batch"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_envelope_is_skipped_without_dropping_the_batch() {
        // Undecodable unsigned bytes fail the very first extraction pass. A
        // batch-level short-circuit would turn this into `Skip`, silently
        // dropping any valid messages delivered alongside it; the per-envelope
        // pass skips only the bad one, so the delivery still resolves to
        // `Messages`.
        let bad = OriginatorEnvelope {
            unsigned_originator_envelope: vec![0xFF; 8],
            proof: None,
        };
        let out = D14nBinding::handle(response(subscribe_response::v1::Response::Envelopes(
            subscribe_response::v1::Envelopes {
                envelopes: vec![bad],
            },
        )));
        match out {
            Inbound::Messages { group, welcome } => {
                assert!(group.is_empty());
                assert!(welcome.is_empty());
            }
            _ => panic!("expected Messages, not a dropped batch"),
        }
    }
}
