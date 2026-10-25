//! The publish count gate over native gRPC and gRPC-Web.

use super::signatures::{rejection, stored_rows};
use super::*;
use base64::Engine;
use support::native::{framed, raw_unary};
use tonic::Status;
use xmtp_configuration::MAX_PUBLISH_ENVELOPES;

pub(super) const PUBLISH: &str = "/xmtp.backend.v1.PublishService/Publish";

/// `count` empty envelopes followed by `tail`, as a gRPC message.
pub(super) fn empty_envelopes(count: usize, tail: &[u8]) -> Vec<u8> {
    let mut message = [0x0a, 0x00].repeat(count);
    message.extend_from_slice(tail);
    framed(&message)
}

/// Post `body` as a gRPC-Web publish, as binary or as base64 text, and return
/// the status from the response headers or from its trailer frame.
async fn grpc_web_publish(
    server: &TestServer,
    body: &[u8],
    text: bool,
) -> support::TestResult<Status> {
    let (content_type, body) = if text {
        (
            "application/grpc-web-text+proto",
            base64::engine::general_purpose::STANDARD
                .encode(body)
                .into_bytes(),
        )
    } else {
        ("application/grpc-web+proto", body.to_vec())
    };
    let response = xmtp_common::http::client()?
        .post(format!("{}{PUBLISH}", server.url))
        .header("content-type", content_type)
        .header("accept", "application/grpc-web+proto")
        .body(body)
        .send()
        .await?;
    if let Some(status) = Status::from_header_map(response.headers()) {
        return Ok(status);
    }
    let body = response.bytes().await?;
    let mut rest = &body[..];
    while let [flag, a, b, c, d, after @ ..] = rest {
        let (payload, next) = after
            .split_at_checked(u32::from_be_bytes([*a, *b, *c, *d]) as usize)
            .ok_or("truncated gRPC-Web frame")?;
        if flag & 0x80 != 0 {
            let mut trailers = http::HeaderMap::new();
            for line in std::str::from_utf8(payload)?.split('\n') {
                if let Some((name, value)) = line.trim_end_matches('\r').split_once(':') {
                    trailers.insert(
                        http::HeaderName::from_bytes(name.trim().as_bytes())?,
                        value.trim().parse()?,
                    );
                }
            }
            return Ok(Status::from_header_map(&trailers).ok_or("no grpc-status")?);
        }
        rest = next;
    }
    Err("no trailer frame".into())
}

/// Tonic decodes every message of a unary body and hands the handler the
/// first, so an empty request followed by any number of entries succeeds.
#[xmtp_common::test(unwrap_try = true)]
async fn publish_second_message_is_rejected_before_decode() {
    let server = TestServer::new(|_| {}).await?;
    let body = [framed(&[]), empty_envelopes(16_384, &[])].concat();
    let error = raw_unary(&server, PUBLISH, &[], body, true).await?;
    assert_eq!(error.message(), "request carries more than one message");
    assert_eq!(rejection(error)?, (None, Reason::TooLarge));
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-304
async fn publish_count_is_rejected_before_decode_over_grpc_web() {
    let server = TestServer::new(|_| {}).await?;
    let over = empty_envelopes(16_384, &[0x0a, 0x05]);
    for body in [over.clone(), [framed(&[]), over].concat()] {
        for text in [false, true] {
            let error = grpc_web_publish(&server, &body, text).await?;
            assert_eq!(rejection(error)?, (None, Reason::TooLarge), "text: {text}");
        }
    }
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}

/// A request within the bound reaches the decoder unchanged, so a malformed
/// one keeps the decoder's status.
#[xmtp_common::test(unwrap_try = true)]
async fn publish_count_at_bound_keeps_decoder_status() {
    let server = TestServer::new(|_| {}).await?;
    for tail in [&[0xff][..], &[0x0a, 0x05]] {
        let body = empty_envelopes(MAX_PUBLISH_ENVELOPES, tail);
        let error = raw_unary(&server, PUBLISH, &[], body, true).await?;
        assert_eq!(error.code(), Code::Internal, "{error:?}");
        assert!(
            error
                .message()
                .starts_with("failed to decode Protobuf message")
        );
    }
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}

/// Tonic refuses a length over its limit from the prefix alone, and a
/// compressed body from its headers, since no gated service accepts
/// compression. The body stays open, so a gate that waited for more bytes
/// would never answer.
#[xmtp_common::test(unwrap_try = true)]
async fn over_limit_prefix_keeps_the_transport_status() {
    let server = TestServer::new(|_| {}).await?;
    let length = server.backend.config.limits.max_request_bytes as u32 + 1;
    let mut prefix = vec![0];
    prefix.extend_from_slice(&length.to_be_bytes());
    let error = raw_unary(&server, PUBLISH, &[], prefix, false).await?;
    assert_eq!(error.code(), Code::OutOfRange, "{error:?}");
    let compressed = [("grpc-encoding", "gzip")];
    let error = raw_unary(&server, PUBLISH, &compressed, vec![1, 0, 0, 0, 9], false).await?;
    assert_eq!(error.code(), Code::Unimplemented, "{error:?}");
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}
