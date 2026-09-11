use crate::test_support as support;

use crate::api;
use prost::Message;
use reqwest::header::{HeaderMap, HeaderValue};
use support::TestServer;
use tonic::{Code, Request};
use tonic_health::pb::{HealthCheckRequest, health_check_response, health_client::HealthClient};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

#[path = "https_ingress.rs"]
mod https_ingress;

struct GrpcWebResponse {
    status: reqwest::StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

struct GrpcWebFrames {
    data: Vec<Vec<u8>>,
    trailers: std::collections::HashMap<String, String>,
}

impl GrpcWebResponse {
    fn grpc_status(&self) -> Option<i32> {
        self.headers
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .or_else(|| {
                self.trailers().ok().and_then(|trailers| {
                    trailers
                        .get("grpc-status")
                        .and_then(|value| value.parse().ok())
                })
            })
    }

    fn has_grpc_status_details(&self) -> bool {
        self.headers.contains_key("grpc-status-details-bin")
            || self
                .trailers()
                .is_ok_and(|trailers| trailers.contains_key("grpc-status-details-bin"))
    }

    fn data_frames(&self) -> support::TestResult<Vec<Vec<u8>>> {
        Ok(decode_frames(&self.body)?.data)
    }

    fn trailers(&self) -> support::TestResult<std::collections::HashMap<String, String>> {
        Ok(decode_frames(&self.body)?.trailers)
    }
}

pub(super) fn encode_frame(message: impl Message) -> Vec<u8> {
    let bytes = message.encode_to_vec();
    let mut frame = Vec::with_capacity(5 + bytes.len());
    frame.push(0);
    frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&bytes);
    frame
}

fn decode_frames(body: &[u8]) -> support::TestResult<GrpcWebFrames> {
    let mut data = Vec::new();
    let mut trailers = std::collections::HashMap::new();
    let mut offset = 0;
    while offset < body.len() {
        if body.len() - offset < 5 {
            return Err("incomplete gRPC-Web frame header".into());
        }
        let flags = body[offset];
        let length = u32::from_be_bytes(body[offset + 1..offset + 5].try_into()?) as usize;
        offset += 5;
        if body.len() - offset < length {
            return Err("incomplete gRPC-Web frame payload".into());
        }
        let payload = &body[offset..offset + length];
        offset += length;
        if flags & 0x80 != 0 {
            for line in payload.split(|byte| *byte == b'\n') {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                if let Some(separator) = line.iter().position(|byte| *byte == b':') {
                    let (key, value) = line.split_at(separator);
                    let value = &value[1..];
                    trailers.insert(
                        String::from_utf8(key.to_ascii_lowercase())?,
                        String::from_utf8(value.strip_prefix(b" ").unwrap_or(value).to_vec())?,
                    );
                }
            }
        } else {
            data.push(payload.to_vec());
        }
    }
    Ok(GrpcWebFrames { data, trailers })
}

async fn grpc_web_post(
    server: &TestServer,
    path: &str,
    body: Vec<u8>,
    headers: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> support::TestResult<GrpcWebResponse> {
    let client = xmtp_common::http::client()?;
    let mut request_headers = HeaderMap::new();
    request_headers.insert(
        "content-type",
        HeaderValue::from_static("application/grpc-web+proto"),
    );
    request_headers.insert(
        "accept",
        HeaderValue::from_static("application/grpc-web+proto"),
    );
    for (name, value) in headers {
        request_headers.insert(name, HeaderValue::from_static(value));
    }
    let response = client
        .post(format!("{server_url}{path}", server_url = server.url))
        .headers(request_headers)
        .body(body)
        .send()
        .await?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await?.to_vec();
    Ok(GrpcWebResponse {
        status,
        headers,
        body,
    })
}

#[xmtp_common::test(unwrap_try = true)]
async fn health_check_reports_serving() {
    let server = TestServer::new(|_| {}).await?;
    let mut client = HealthClient::new(server.channel.clone());
    let response = client
        .check(Request::new(HealthCheckRequest {
            service: String::new(),
        }))
        .await?
        .into_inner();
    assert_eq!(
        response.status(),
        health_check_response::ServingStatus::Serving
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn http2_advertises_the_configured_stream_limit() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const STREAM_LIMIT: usize = 7;
    const SETTINGS_FRAME: u8 = 4;
    const MAX_CONCURRENT_STREAMS: u16 = 3;
    const SETTINGS_ENTRY_BYTES: usize = 6;
    let server = TestServer::new(|config| config.limits.max_http2_streams = STREAM_LIMIT).await?;
    let address = server.url.strip_prefix("http://").unwrap();
    let mut connection = tokio::net::TcpStream::connect(address).await?;
    connection
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
        .await?;
    connection
        .write_all(&[0, 0, 0, SETTINGS_FRAME, 0, 0, 0, 0, 0])
        .await?;
    let mut header = [0; 9];
    connection.read_exact(&mut header).await?;
    assert_eq!(header[3], SETTINGS_FRAME);
    let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
    let mut payload = vec![0; length];
    connection.read_exact(&mut payload).await?;
    let advertised = payload
        .chunks_exact(SETTINGS_ENTRY_BYTES)
        .find_map(|setting| {
            (u16::from_be_bytes(setting[..2].try_into().unwrap()) == MAX_CONCURRENT_STREAMS)
                .then(|| u32::from_be_bytes(setting[2..].try_into().unwrap()))
        });
    assert_eq!(advertised, Some(STREAM_LIMIT as u32));
    drop(connection);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deprecated_legacy_path_returns_unimplemented() {
    let server = TestServer::new(|_| {}).await?;
    let response = grpc_web_post(
        &server,
        "/xmtp.mls.api.v1.MlsApi/SubscribeGroupMessages",
        encode_frame(api::QueryNewestRequest::default()),
        [],
    )
    .await?;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert_eq!(response.grpc_status(), Some(Code::Unimplemented as i32));
    assert!(response.data_frames()?.is_empty());
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn grpc_web_unary_matches_native_grpc_and_accepts_headers() {
    let server = TestServer::new(|_| {}).await?;
    let envelope = inline_welcome_envelope([7; 32]);
    let meta = server.publish(vec![envelope.clone()]).await?.remove(0);
    let newest = api::QueryNewestRequest {
        topics: vec![meta.topic.clone().unwrap()],
        include_full_envelope: true,
    };
    let mut request = Request::new(newest.clone());
    request
        .metadata_mut()
        .insert("authorization", "Bearer test".parse()?);
    request
        .metadata_mut()
        .insert("x-app-version", "transport-test".parse()?);
    request
        .metadata_mut()
        .insert("x-libxmtp-version", "transport-test".parse()?);
    let native = server.query().query_newest(request).await?.into_inner();

    let response = grpc_web_post(
        &server,
        "/xmtp.backend.v1.QueryService/QueryNewest",
        encode_frame(newest),
        [
            ("authorization", "Bearer test"),
            ("x-app-version", "transport-test"),
            ("x-libxmtp-version", "transport-test"),
        ],
    )
    .await?;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert_eq!(response.grpc_status(), Some(Code::Ok as i32));
    let frames = response.data_frames()?;
    assert_eq!(frames.len(), 1);
    let web = api::QueryNewestResponse::decode(frames[0].as_slice())?;
    assert_eq!(web, native);
    assert_eq!(web.results[0].envelope, Some(envelope));
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn cors_preflight_allows_client_headers_and_exposes_status_details() {
    let server = TestServer::new(|_| {}).await?;
    let client = xmtp_common::http::client()?;
    let response = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{}/xmtp.backend.v1.QueryService/QueryNewest", server.url),
        )
        .header("origin", "https://app.example")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type,x-app-version,x-libxmtp-version",
        )
        .send()
        .await?;
    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|value| value.to_str().ok()),
        Some("*")
    );
    let allowed = response
        .headers()
        .get("access-control-allow-headers")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    for header in [
        "authorization",
        "content-type",
        "x-app-version",
        "x-libxmtp-version",
    ] {
        assert!(allowed.split(',').any(|value| value.trim() == header));
    }

    let actual = grpc_web_post(
        &server,
        "/xmtp.mls.api.v1.MlsApi/SubscribeGroupMessages",
        encode_frame(api::QueryNewestRequest::default()),
        [
            ("origin", "https://app.example"),
            ("authorization", "Bearer test"),
            ("x-app-version", "transport-test"),
            ("x-libxmtp-version", "transport-test"),
        ],
    )
    .await?;
    let exposed = actual
        .headers
        .get("access-control-expose-headers")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    for header in ["grpc-status", "grpc-message", "grpc-status-details-bin"] {
        assert!(exposed.split(',').any(|value| value.trim() == header));
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn oversized_wire_request_is_rejected_without_publish_detail_or_write() {
    let envelope = inline_welcome_envelope([8; 32]);
    let request = api::PublishRequest {
        envelopes: vec![envelope],
    };
    let server = TestServer::new(|config| {
        config.limits.max_request_bytes = 66_560;
        config.limits.max_envelope_bytes = 1_024;
    })
    .await?;
    let mut oversized = request.encode_to_vec();
    oversized.extend_from_slice(&[0x92, 0x3e, 0xf0, 0xa2, 0x04]);
    oversized.extend(std::iter::repeat_n(0, 70_000));
    let response = grpc_web_post(
        &server,
        "/xmtp.backend.v1.PublishService/Publish",
        encode_raw_frame(oversized),
        [],
    )
    .await?;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert_eq!(response.grpc_status(), Some(Code::OutOfRange as i32));
    assert!(!response.has_grpc_status_details());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 0);
    server.stop().await?;
}

fn encode_raw_frame(bytes: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + bytes.len());
    frame.push(0);
    frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&bytes);
    frame
}

#[xmtp_common::test(unwrap_try = true)]
async fn oversized_publish_response_reports_transport_error_after_commit() {
    let Some(metrics) = support::metrics::isolated(
        "server::tests::transport::oversized_publish_response_reports_transport_error_after_commit",
    ) else {
        return;
    };
    let server = TestServer::new(|config| {
        config.limits.max_envelope_bytes = 1_024;
        config.limits.max_response_bytes = 66_560;
    })
    .await?;
    let envelopes: Vec<_> = (0_u64..800)
        .map(|id| {
            let mut installation_key = [0_u8; 32];
            installation_key[..8].copy_from_slice(&id.to_le_bytes());
            inline_welcome_envelope(installation_key)
        })
        .collect();
    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: envelopes.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::OutOfRange);
    let retry = server.publish(vec![envelopes[0].clone()]).await?;
    assert_eq!(retry.len(), 1);
    let fetched = server
        .query()
        .query_newest(api::QueryNewestRequest {
            topics: vec![retry[0].topic.clone().unwrap()],
            include_full_envelope: true,
        })
        .await?
        .into_inner()
        .results
        .pop()?;
    assert_eq!(fetched.meta, Some(retry[0].clone()));
    assert_eq!(fetched.envelope, Some(envelopes[0].clone()));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 800);
    assert_eq!(
        support::metrics::value(
            &metrics,
            "xmtp_publish_envelopes_total",
            &[("outcome", "rejected")]
        ),
        800.0
    );
    assert_eq!(
        support::metrics::value(
            &metrics,
            "xmtp_publish_envelopes_total",
            &[("outcome", "duplicate")]
        ),
        1.0
    );
    assert_eq!(
        support::metrics::value(
            &metrics,
            "xmtp_publish_envelopes_total",
            &[("outcome", "stored")]
        ),
        0.0
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn grpc_web_preserves_structured_publish_error_details() {
    let server = TestServer::new(|_| {}).await?;
    let request = api::PublishRequest {
        envelopes: vec![
            inline_welcome_envelope([75; 32]),
            api::ClientEnvelope::default(),
        ],
    };
    let response = grpc_web_post(
        &server,
        "/xmtp.backend.v1.PublishService/Publish",
        encode_frame(request),
        [("origin", "https://app.example")],
    )
    .await?;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert_eq!(response.grpc_status(), Some(Code::InvalidArgument as i32));
    let frames = decode_frames(&response.body)?;
    assert!(frames.data.is_empty());
    let mut headers = response.headers.clone();
    if !headers.contains_key("grpc-status-details-bin") {
        let details = frames
            .trailers
            .get("grpc-status-details-bin")
            .expect("gRPC-Web trailers must carry structured details");
        headers.insert("grpc-status-details-bin", HeaderValue::from_str(details)?);
    }
    let metadata = tonic::metadata::MetadataMap::from_headers(headers);
    let decoded = tonic_types::pb::Status::decode(
        metadata
            .get_bin("grpc-status-details-bin")
            .unwrap()
            .to_bytes()?,
    )?;
    assert_eq!(decoded.code, Code::InvalidArgument as i32);
    assert_eq!(decoded.details.len(), 1);
    assert_eq!(
        decoded.details[0].type_url,
        "type.googleapis.com/xmtp.backend.v1.PublishError"
    );
    let detail = api::PublishError::decode(decoded.details[0].value.as_slice())?;
    assert_eq!(detail.index, Some(1));
    assert_eq!(
        detail.reason(),
        api::publish_error::Reason::MalformedPayload
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 0);
    server.stop().await?;
}
