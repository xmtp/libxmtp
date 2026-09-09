use super::*;
use futures::{FutureExt, stream};
use http_body_util::{BodyExt, Full, StreamBody};
use std::{convert::Infallible, io};
use tower::service_fn;
use xmtp_logging::{Level, test_logging::LogCapture};

fn request(body: Body) -> Request<Body> {
    Request::post("/xmtp.backend.v1.QueryService/Query")
        .header(CONTENT_TYPE, "application/grpc")
        .header("authorization", "secret-auth-token")
        .header("x-request-id", "caller-controlled-id")
        .body(body)
        .unwrap()
}

fn events(capture: &LogCapture) -> Vec<serde_json::Value> {
    capture
        .output()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[xmtp_common::test(unwrap_try = true)]
async fn completion_counts_consumed_frames_and_uses_its_own_request_id() {
    let capture = LogCapture::new(Level::Info);
    let body = StreamBody::new(stream::iter(vec![
        Ok::<_, Infallible>(Frame::data(Bytes::from_static(&[0, 0, 0, 0, 7]))),
        Ok(Frame::data(Bytes::from_static(b"payload"))),
        Ok(Frame::trailers(http::HeaderMap::new())),
    ]));
    let mut service =
        GrpcTelemetryLayer(true).layer(service_fn(|request: Request<Body>| async move {
            let id = request.extensions().get::<RequestId>().unwrap().0;
            assert_eq!(request.into_body().collect().await?.to_bytes().len(), 12);
            Ok::<_, tonic::Status>(
                Response::builder()
                    .header("x-observed-id", id.to_string())
                    .body(Body::new(Full::new(Bytes::from_static(b"response"))))
                    .unwrap(),
            )
        }));
    let mut request = request(Body::new(body));
    request
        .headers_mut()
        .insert("content-length", "999999".parse()?);
    let response =
        tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request)).await?;
    assert!(events(&capture).is_empty());
    let id = response.headers()["x-request-id"].to_str()?.to_owned();
    assert_eq!(uuid::Uuid::parse_str(&id)?.get_version_num(), 4);
    assert_eq!(response.headers()["x-observed-id"], id);
    response.into_body().collect().await?;
    let logs = events(&capture);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["request_id"], id);
    assert_eq!(logs[0]["method"], "/xmtp.backend.v1.QueryService/Query");
    assert_eq!(logs[0]["request_size_bytes"], 12);
    assert_eq!(logs[0]["response_size_bytes"], 8);
    assert!(logs[0]["duration_ms"].is_u64());
    assert!(!capture.output().contains("secret-auth-token"));
    assert!(!capture.output().contains("caller-controlled-id"));
    assert!(!capture.output().contains("payload"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn streaming_completion_waits_for_body_drop_and_counts_later_input() {
    let capture = LogCapture::new(Level::Info);
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, Infallible>>(2);
    let body = Body::new(StreamBody::new(
        tokio_stream::wrappers::ReceiverStream::new(receiver),
    ));
    let mut service =
        GrpcTelemetryLayer(true).layer(service_fn(|request: Request<Body>| async move {
            Ok::<_, Infallible>(Response::new(request.into_body()))
        }));
    let response =
        tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request(body)))
            .await?;
    assert!(events(&capture).is_empty());
    let mut body = response.into_body();
    sender
        .send(Ok(Frame::data(Bytes::from_static(b"first"))))
        .await?;
    assert_eq!(body.frame().await.unwrap()?.into_data().unwrap().len(), 5);
    assert!(events(&capture).is_empty());
    sender
        .send(Ok(Frame::data(Bytes::from_static(b"second"))))
        .await?;
    assert_eq!(body.frame().await.unwrap()?.into_data().unwrap().len(), 6);
    assert!(events(&capture).is_empty());
    sender
        .send(Ok(Frame::data(Bytes::from_static(b"not emitted"))))
        .await?;
    drop(body);
    assert_eq!(events(&capture).len(), 1);
    assert_eq!(events(&capture)[0]["request_size_bytes"], 11);
    assert_eq!(events(&capture)[0]["response_size_bytes"], 11);
}

#[xmtp_common::test(unwrap_try = true)]
async fn future_errors_and_cancellation_each_complete_once_before_headers() {
    let capture = LogCapture::new(Level::Info);
    let mut failing =
        GrpcTelemetryLayer(true).layer(service_fn(|request: Request<Body>| async move {
            request.into_body().collect().await.unwrap();
            Err::<Response<Body>, _>(io::Error::other("failed before headers"))
        }));
    let result = tracing::dispatcher::with_default(&capture.dispatch(), || {
        failing.call(request(Body::new(Full::new(Bytes::from_static(b"input")))))
    })
    .await;
    assert!(result.is_err());
    assert_eq!(events(&capture).len(), 1);
    assert_eq!(events(&capture)[0]["request_size_bytes"], 5);
    assert_eq!(events(&capture)[0]["response_size_bytes"], 0);

    let mut canceled =
        GrpcTelemetryLayer(true).layer(service_fn(|request: Request<Body>| async move {
            request.into_body().collect().await.unwrap();
            std::future::pending::<Result<Response<Body>, Infallible>>().await
        }));
    let mut future = tracing::dispatcher::with_default(&capture.dispatch(), || {
        canceled.call(request(Body::new(Full::new(Bytes::from_static(b"later")))))
    });
    assert!(future.as_mut().now_or_never().is_none());
    assert_eq!(events(&capture).len(), 1);
    drop(future);
    assert_eq!(events(&capture).len(), 2);
    assert_eq!(events(&capture)[1]["request_size_bytes"], 5);
    let unpolled = tracing::dispatcher::with_default(&capture.dispatch(), || {
        canceled.call(request(Body::empty()))
    });
    drop(unpolled);
    assert_eq!(events(&capture).len(), 3);
}

#[xmtp_common::test(unwrap_try = true)]
async fn response_body_failure_does_not_log_again_when_dropped() {
    let capture = LogCapture::new(Level::Info);
    let mut service = GrpcTelemetryLayer(true).layer(service_fn(|_: Request<Body>| async move {
        let body = StreamBody::new(stream::iter(vec![
            Ok(Frame::data(Bytes::from_static(b"partial"))),
            Err::<Frame<Bytes>, _>(tonic::Status::internal("body failed")),
        ]));
        Ok::<_, Infallible>(Response::new(Body::new(body)))
    }));
    let response = tracing::dispatcher::with_default(&capture.dispatch(), || {
        service.call(request(Body::empty()))
    })
    .await?;
    assert!(events(&capture).is_empty());
    let mut body = response.into_body();
    assert_eq!(
        body.frame().await.unwrap()?.into_data().unwrap(),
        Bytes::from_static(b"partial")
    );
    assert!(events(&capture).is_empty());
    assert!(body.frame().await.unwrap().is_err());
    assert_eq!(events(&capture).len(), 1);
    drop(body);
    assert_eq!(events(&capture).len(), 1);
    assert_eq!(events(&capture)[0]["response_size_bytes"], 7);
}

#[xmtp_common::test(unwrap_try = true)]
async fn disabled_logger_warn_level_and_preflight_emit_no_completion() {
    for (enabled, level, method) in [
        (false, Level::Info, Method::POST),
        (true, Level::Warn, Method::POST),
        (true, Level::Info, Method::OPTIONS),
    ] {
        let capture = LogCapture::new(level);
        let mut service =
            GrpcTelemetryLayer(enabled).layer(service_fn(|request: Request<Body>| async move {
                Ok::<_, Infallible>(Response::new(request.into_body()))
            }));
        let mut request = request(Body::new(Full::new(Bytes::from_static(b"input"))));
        *request.method_mut() = method;
        let response =
            tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request))
                .await?;
        response.into_body().collect().await?;
        assert!(events(&capture).is_empty());
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn grpc_web_counts_encoded_http_bytes_before_protocol_conversion() {
    let capture = LogCapture::new(Level::Info);
    let inner = service_fn(|request: Request<Body>| async move {
        assert_eq!(
            request.into_body().collect().await?.to_bytes(),
            Bytes::from_static(&[0; 5])
        );
        let mut trailers = http::HeaderMap::new();
        trailers.insert("grpc-status", "0".parse().unwrap());
        let response = StreamBody::new(stream::iter([
            Ok::<_, Infallible>(Frame::data(Bytes::from_static(&[0; 5]))),
            Ok(Frame::trailers(trailers)),
        ]));
        Ok::<_, tonic::Status>(
            Response::builder()
                .header(CONTENT_TYPE, "application/grpc")
                .body(Body::new(response))
                .unwrap(),
        )
    });
    let mut service = GrpcTelemetryLayer(true)
        .layer(tonic_web::GrpcWebLayer::new().layer(GrpcStatusLayer.layer(inner)));
    // The fixture is the base64 representation of an empty five-byte gRPC frame.
    let request = Request::post("/xmtp.backend.v1.QueryService/Query")
        .version(http::Version::HTTP_11)
        .header(CONTENT_TYPE, "application/grpc-web-text+proto")
        .header("accept", "application/grpc-web-text+proto")
        .body(Body::new(Full::new(Bytes::from_static(b"AAAAAAA="))))?;
    let response =
        tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request)).await?;
    assert!(events(&capture).is_empty());
    let response_bytes = response.into_body().collect().await?.to_bytes();
    assert_eq!(events(&capture).len(), 1);
    assert_eq!(events(&capture)[0]["request_size_bytes"], 8);
    assert!(response_bytes.len() > 8);
    assert_eq!(
        events(&capture)[0]["response_size_bytes"],
        response_bytes.len()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn native_updates_log_actual_changes_with_the_request_correlation() {
    use crate::{api, test_support};
    use prost::Message;
    use tonic::codec::Codec;

    let server = test_support::TestServer::new(|_| {}).await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[81; 32]);
    let absent = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[82; 32]);
    for (enabled, level, expected_logs) in [
        (true, Level::Info, 4),
        (false, Level::Info, 3),
        (true, Level::Warn, 0),
    ] {
        let capture = LogCapture::new(level);
        let mut codec =
            tonic_prost::ProstCodec::<api::SubscribeRequest, api::SubscribeResponse>::default();
        let (sender, receiver) =
            tokio::sync::mpsc::channel::<Result<api::SubscribeRequest, tonic::Status>>(4);
        let body = tonic::codec::EncodeBody::new_client(
            codec.encoder(),
            tokio_stream::wrappers::ReceiverStream::new(receiver),
            None,
            None,
        );
        let request = Request::post("/xmtp.backend.v1.SubscriptionService/Subscribe")
            .header(CONTENT_TYPE, "application/grpc")
            .body(Body::new(body))?;
        let inner = api::subscription_service_server::SubscriptionServiceServer::new(
            server.backend.clone(),
        );
        let mut service = GrpcTelemetryLayer(enabled).layer(inner);
        let response =
            tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request))
                .await?;
        let id = response.headers()["x-request-id"].to_str()?.to_owned();
        let mut output = tonic::Streaming::new_response(
            codec.decoder(),
            response.into_body(),
            http::StatusCode::OK,
            None,
            None,
        );
        assert!(matches!(
            output.message().await?.unwrap().response,
            Some(api::subscribe_response::Response::Started(_))
        ));
        let updates = [
            api::subscribe_request::Update {
                id: 1,
                adds: vec![test_support::query_topic(topic.clone(), 0)],
                removes: vec![absent.clone()],
            },
            api::subscribe_request::Update {
                id: 2,
                adds: vec![test_support::query_topic(topic.clone(), 100)],
                removes: vec![absent.clone()],
            },
            api::subscribe_request::Update {
                id: 3,
                adds: vec![],
                removes: vec![topic.clone()],
            },
        ];
        let mut wire_bytes = 0;
        for update in updates {
            let expected_id = update.id;
            let request = api::SubscribeRequest {
                request: Some(api::subscribe_request::Request::Update(update)),
            };
            wire_bytes += request.encoded_len() + 5;
            sender.send(Ok(request)).await?;
            assert!(matches!(output.message().await?.unwrap().response,
                Some(api::subscribe_response::Response::Applied(applied)) if applied.id == expected_id));
        }
        drop(sender);
        assert!(output.message().await?.is_none());
        let logs = events(&capture);
        assert_eq!(logs.len(), expected_logs);
        if expected_logs == 0 {
            continue;
        }
        for (index, (added, removed)) in [(1, 0), (0, 0), (0, 1)].into_iter().enumerate() {
            assert_eq!(logs[index]["message"], "subscription interests updated");
            assert_eq!(logs[index]["request_id"], id);
            assert_eq!(logs[index]["update_id"], index + 1);
            assert_eq!(logs[index]["added_topics"], added);
            assert_eq!(logs[index]["removed_topics"], removed);
            assert!(
                logs[index]["spans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|span| span["request_id"] == id)
            );
        }
        if enabled {
            assert_eq!(logs[3]["request_size_bytes"], wire_bytes);
            assert_eq!(logs[3]["request_id"], id);
            assert_eq!(logs[3]["message"], "gRPC request completed");
        }
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn compressed_native_request_logs_compressed_body_size() {
    use crate::{api, test_support};
    use prost::Message;
    use tonic::codec::{Codec, CompressionEncoding, EncodeBody};

    let server = test_support::TestServer::new(|_| {}).await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[83; 32]);
    let query = api::QueryRequest {
        queries: vec![test_support::query_topic(topic, 0); 100],
        limit: 1,
    };
    let uncompressed_size = query.encoded_len() + 5;
    let mut codec = tonic_prost::ProstCodec::<api::QueryRequest, api::QueryResponse>::default();
    let encoded = EncodeBody::new_client(
        codec.encoder(),
        stream::iter([Ok::<_, tonic::Status>(query)]),
        Some(CompressionEncoding::Gzip),
        None,
    )
    .collect()
    .await?
    .to_bytes();
    assert!(encoded.len() < uncompressed_size);
    let wire_size = encoded.len();
    let inner = api::query_service_server::QueryServiceServer::new(server.backend.clone())
        .accept_compressed(CompressionEncoding::Gzip)
        .send_compressed(CompressionEncoding::Gzip);
    let mut service = GrpcTelemetryLayer(true).layer(inner);
    let capture = LogCapture::new(Level::Info);
    let mut request = request(Body::new(Full::new(encoded)));
    request
        .headers_mut()
        .insert("grpc-encoding", "gzip".parse()?);
    request
        .headers_mut()
        .insert("grpc-accept-encoding", "gzip".parse()?);
    let response =
        tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request)).await?;
    assert_eq!(response.headers()["grpc-encoding"], "gzip");
    let response_bytes = response.into_body().collect().await?.to_bytes();
    assert_eq!(events(&capture).len(), 1);
    assert_eq!(events(&capture)[0]["request_size_bytes"], wire_size);
    assert_eq!(
        events(&capture)[0]["response_size_bytes"],
        response_bytes.len()
    );
    assert_eq!(response_bytes[0], 1);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
fn response_statuses_and_stream_types_have_bounded_complete_metrics() {
    use crate::test_support::metrics::value;
    for (path, content_type, header, trailer, expected, kind) in [
        (
            "/xmtp.backend.v1.QueryService/Query",
            "application/grpc",
            None,
            Some("0"),
            "OK",
            "unary",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc",
            None,
            Some("5"),
            "NotFound",
            "unary",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc",
            Some("7"),
            None,
            "PermissionDenied",
            "unary",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc",
            Some("7"),
            Some("0"),
            "PermissionDenied",
            "unary",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc-web+proto",
            None,
            Some("3"),
            "InvalidArgument",
            "unary",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc-web-text+proto",
            None,
            Some("3"),
            "InvalidArgument",
            "unary",
        ),
        (
            "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
            "application/grpc",
            None,
            Some("0"),
            "OK",
            "server_stream",
        ),
        (
            "/xmtp.backend.v1.SubscriptionService/Subscribe",
            "application/grpc",
            None,
            Some("14"),
            "Unavailable",
            "bidi_stream",
        ),
        (
            "/xmtp.backend.v1.QueryService/Get",
            "application/grpc",
            None,
            None,
            "Unknown",
            "unary",
        ),
    ] {
        let recorder = crate::telemetry::recorder_builder()?.build_recorder();
        let handle = recorder.handle();
        let capture = LogCapture::new(Level::Info);
        metrics::with_local_recorder(&recorder, || {
            complete_immediately(async {
                crate::telemetry::describe();
                let inner = service_fn(move |_: Request<Body>| async move {
                    let mut frames = Vec::new();
                    if header.is_none() || trailer.is_some() {
                        frames.push(Ok::<_, Infallible>(Frame::data(Bytes::from_static(
                            &[0; 5],
                        ))));
                    }
                    if let Some(code) = trailer {
                        let mut trailers = http::HeaderMap::new();
                        trailers.insert("grpc-status", code.parse().unwrap());
                        frames.push(Ok(Frame::trailers(trailers)));
                    }
                    let mut response = Response::builder().header(CONTENT_TYPE, "application/grpc");
                    if let Some(code) = header {
                        response = response.header("grpc-status", code);
                    }
                    Ok::<_, Infallible>(
                        response
                            .body(Body::new(StreamBody::new(stream::iter(frames))))
                            .unwrap(),
                    )
                });
                let mut service = GrpcTelemetryLayer(true)
                    .layer(tonic_web::GrpcWebLayer::new().layer(GrpcStatusLayer.layer(inner)));
                let request = Request::post(path)
                    .version(if content_type == "application/grpc" {
                        http::Version::HTTP_2
                    } else {
                        http::Version::HTTP_11
                    })
                    .header(CONTENT_TYPE, content_type)
                    .header("accept", content_type)
                    .body(Body::empty())
                    .unwrap();
                let response = tracing::dispatcher::with_default(&capture.dispatch(), || {
                    service.call(request)
                })
                .await
                .unwrap();
                assert!(response.headers().contains_key("x-request-id"));
                response.into_body().collect().await.unwrap();
            })
        });
        let (service, method) = path.trim_start_matches('/').split_once('/')?;
        let labels = [
            ("grpc_type", kind),
            ("grpc_service", service),
            ("grpc_method", method),
        ];
        let mut completed = labels.to_vec();
        completed.push(("grpc_code", expected));
        assert_eq!(value(&handle, "grpc_server_started_total", &labels), 1.0);
        assert_eq!(value(&handle, "grpc_server_handled_total", &completed), 1.0);
        assert_eq!(
            value(&handle, "grpc_server_handling_seconds_count", &completed),
            1.0
        );
        assert_eq!(value(&handle, "grpc_server_in_flight", &labels), 0.0);
        let logs = events(&capture);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["grpc_code"], expected);
        assert!(logs[0].get("trace_id").is_none());
        assert_eq!(
            value(&handle, "grpc_server_response_bytes_total", &labels),
            logs[0]["response_size_bytes"].as_u64()? as f64
        );
        for metric in [
            "grpc_server_started_total",
            "grpc_server_handled_total",
            "grpc_server_handling_seconds",
            "grpc_server_in_flight",
            "grpc_server_request_bytes_total",
            "grpc_server_response_bytes_total",
        ] {
            assert!(handle.render().contains(&format!("# HELP {metric} ")));
            assert!(handle.render().contains(&format!("# TYPE {metric} ")));
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn dropping_a_body_records_cancelled_once_even_when_logging_is_disabled() {
    use crate::test_support::metrics::value;
    let recorder = crate::telemetry::recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    let capture = LogCapture::new(Level::Info);
    metrics::with_local_recorder(&recorder, || {
        complete_immediately(async {
            let inner = service_fn(|_: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::new(StreamBody::new(
                    stream::pending::<Result<Frame<Bytes>, Infallible>>(),
                ))))
            });
            let mut service = GrpcTelemetryLayer(false).layer(GrpcStatusLayer.layer(inner));
            let response = tracing::dispatcher::with_default(&capture.dispatch(), || {
                service.call(request(Body::empty()))
            })
            .await
            .unwrap();
            assert_eq!(value(&handle, "grpc_server_in_flight", &[]), 1.0);
            drop(response);
            assert_eq!(value(&handle, "grpc_server_in_flight", &[]), 0.0);
        })
    });
    assert_eq!(
        value(
            &handle,
            "grpc_server_handled_total",
            &[("grpc_code", "Cancelled")]
        ),
        1.0
    );
    assert_eq!(
        value(
            &handle,
            "grpc_server_handling_seconds_count",
            &[("grpc_code", "Cancelled")]
        ),
        1.0
    );
    assert!(events(&capture).is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn health_and_preflight_are_excluded_and_unknown_paths_share_one_label_set() {
    use crate::test_support::metrics::value;
    const UNKNOWN_PATHS: usize = 1_000;
    let recorder = crate::telemetry::recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    let capture = LogCapture::new(Level::Info);
    metrics::with_local_recorder(&recorder, || {
        complete_immediately(async {
            let inner = service_fn(|_: Request<Body>| async {
                Ok::<_, Infallible>(
                    Response::builder()
                        .header("grpc-status", "0")
                        .body(Body::empty())
                        .unwrap(),
                )
            });
            let mut service = GrpcTelemetryLayer(true).layer(GrpcStatusLayer.layer(inner));
            for method in [Method::POST, Method::OPTIONS] {
                let request = Request::builder()
                    .method(method)
                    .uri("/grpc.health.v1.Health/Check")
                    .header(CONTENT_TYPE, "application/grpc")
                    .body(Body::empty())
                    .unwrap();
                let response = tracing::dispatcher::with_default(&capture.dispatch(), || {
                    service.call(request)
                })
                .await
                .unwrap();
                if response.headers().contains_key("x-request-id") {
                    assert_eq!(events(&capture).len(), 1);
                }
                response.into_body().collect().await.unwrap();
            }
            assert!(
                handle
                    .render()
                    .lines()
                    .all(|line| !line.starts_with("grpc_server_"))
            );
            assert_eq!(events(&capture).len(), 1);
            assert_eq!(events(&capture)[0]["grpc_code"], "OK");
            service.enabled = false;
            for index in 0..UNKNOWN_PATHS {
                let request = Request::post(format!("/scanner-{index}/method-{index}"))
                    .header(CONTENT_TYPE, "application/grpc")
                    .body(Body::empty())
                    .unwrap();
                service
                    .call(request)
                    .await
                    .unwrap()
                    .into_body()
                    .collect()
                    .await
                    .unwrap();
            }
        })
    });
    let labels = [
        ("grpc_service", "unknown"),
        ("grpc_method", "unknown"),
        ("grpc_type", "unary"),
    ];
    assert_eq!(
        value(&handle, "grpc_server_started_total", &labels),
        UNKNOWN_PATHS as f64
    );
    assert_eq!(
        value(&handle, "grpc_server_handled_total", &labels),
        UNKNOWN_PATHS as f64
    );
    for metric in [
        "grpc_server_started_total",
        "grpc_server_handled_total",
        "grpc_server_handling_seconds_count",
        "grpc_server_in_flight",
        "grpc_server_request_bytes_total",
        "grpc_server_response_bytes_total",
    ] {
        assert_eq!(
            handle
                .render()
                .lines()
                .filter(|line| line.starts_with(&format!("{metric}{{")))
                .count(),
            1,
            "{metric}"
        );
    }
    assert!(!handle.render().contains("scanner-"));
}

#[xmtp_common::test(unwrap_try = true)]
fn incoming_trace_parent_and_server_attributes_are_preserved_with_optional_export() {
    use xmtp_logging::test_logging::with_trace_layer;
    const TRACE_ID: &str = "12345678901234567890123456789012";
    const PARENT_ID: &str = "1234567890123456";
    for enabled in [false, true] {
        let recorder = crate::telemetry::recorder_builder()?.build_recorder();
        let handle = recorder.handle();
        let (_, spans) = with_trace_layer(enabled, || {
            metrics::with_local_recorder(&recorder, || {
                complete_immediately(async {
                    let inner = service_fn(move |_: Request<Body>| async {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .header("grpc-status", "0")
                                .body(Body::empty())
                                .unwrap(),
                        )
                    });
                    let mut service = GrpcTelemetryLayer(false).layer(GrpcStatusLayer.layer(inner));
                    let mut request = request(Body::empty());
                    request.headers_mut().insert(
                        "traceparent",
                        format!("00-{TRACE_ID}-{PARENT_ID}-01").parse().unwrap(),
                    );
                    request
                        .headers_mut()
                        .insert("tracestate", "vendor=value".parse().unwrap());
                    service
                        .call(request)
                        .await
                        .unwrap()
                        .into_body()
                        .collect()
                        .await
                        .unwrap();
                })
            })
        });
        assert_eq!(
            crate::test_support::metrics::value(
                &handle,
                "grpc_server_handled_total",
                &[("grpc_code", "OK")]
            ),
            1.0
        );
        if !enabled {
            assert!(spans.is_empty());
            continue;
        }
        let span = spans
            .iter()
            .find(|span| span.name == "xmtp.backend.v1.QueryService/Query")?;
        assert_eq!(span.span_context.trace_id().to_string(), TRACE_ID);
        assert_eq!(span.parent_span_id.to_string(), PARENT_ID);
        assert_eq!(format!("{:?}", span.span_kind), "Server");
        for (key, expected) in [
            ("rpc.system", "grpc"),
            ("rpc.service", "xmtp.backend.v1.QueryService"),
            ("rpc.method", "Query"),
            ("rpc.grpc.status_code", "0"),
        ] {
            assert!(
                span.attributes
                    .iter()
                    .any(|attribute| attribute.key.as_str() == key
                        && attribute.value.as_str() == expected),
                "{key}"
            );
        }
    }
    let capture = LogCapture::new(Level::Info);
    propagation::install();
    let mut service = GrpcTelemetryLayer(true).layer(service_fn(|_: Request<Body>| async {
        Ok::<_, Infallible>(
            Response::builder()
                .header("grpc-status", "0")
                .body(Body::empty())
                .unwrap(),
        )
    }));
    let mut request = request(Body::empty());
    request.headers_mut().insert(
        "traceparent",
        format!("00-{TRACE_ID}-{PARENT_ID}-01").parse()?,
    );
    complete_immediately(tracing::dispatcher::with_default(
        &capture.dispatch(),
        || service.call(request),
    ))?
    .into_body();
    assert_eq!(events(&capture)[0]["trace_id"], TRACE_ID);
}

/// Stub services have no pending work. Poll without nesting an executor inside OTel export.
fn complete_immediately<F: Future>(future: F) -> F::Output {
    future.now_or_never().expect("stub completed in one poll")
}

#[xmtp_common::test(
    unwrap_try = true,
    disable_logging = true,
    flavor = "multi_thread",
    worker_threads = 4
)]
async fn publish_and_subscribe_exclude_topic_and_inbox_bytes_from_all_telemetry() {
    use crate::{api, test_support as support};
    use support::native::{Native, terminal};
    use xmtp_logging::test_logging::{OtlpCollector, string_attribute};
    use xmtp_logging::{FileConfig, ProcessType, Rotation, TelemetryConfig};
    use xmtp_mls_validation::test_utils::{
        identity_envelope, identity_history_with_passkey, inline_welcome_envelope,
    };
    let Some((metrics, address)) = support::metrics::isolated_http(
        "server::telemetry::tests::publish_and_subscribe_exclude_topic_and_inbox_bytes_from_all_telemetry",
    ) else {
        return;
    };
    let fixture = identity_history_with_passkey().await;
    let inbox = fixture.inbox_id.clone();
    let welcome = inline_welcome_envelope([0xab; 32]);
    let topic = xmtp_mls_validation::parse_envelope(welcome.clone())?
        .topic
        .to_vec();
    let logs = support::metrics::LogDirectory::new();
    let collector = OtlpCollector::start().await?;
    let logging = xmtp_logging::XmtpLogging::builder()
        .level(Level::Info)
        .json(true)
        .with_file(Some(FileConfig {
            dir: logs.0.to_string_lossy().into_owned(),
            rotation: Rotation::Never,
            max_files: 1,
            process_type: ProcessType::Main,
            level: Level::Info,
        }))
        .with_telemetry(Some(TelemetryConfig {
            endpoint: Some(collector.endpoint()),
            service_name: Some("xmtp-backend".into()),
            logs: false,
            ..Default::default()
        }))
        .install()?;
    propagation::install();
    let server = support::TestServer::new(|_| {}).await?;
    let metas = server
        .publish(vec![identity_envelope(fixture.history[0].clone()), welcome])
        .await?;
    let mut stream = Native::open(&server).await?;
    stream
        .update(
            1,
            metas
                .iter()
                .map(|meta| support::query_topic(meta.topic.clone().unwrap(), 0))
                .collect(),
            vec![],
        )
        .await?;
    assert!(matches!(
        stream.next().await?,
        api::subscribe_response::Response::Applied(_)
    ));
    assert_eq!(stream.messages(2).await?.len(), 2);
    server
        .publish(vec![identity_envelope(fixture.update)])
        .await?;
    assert_eq!(stream.messages(1).await?.len(), 1);
    let topics: Vec<_> = metas
        .iter()
        .map(|meta| meta.topic.clone().unwrap())
        .collect();
    server
        .query()
        .query(api::QueryRequest {
            queries: topics
                .iter()
                .cloned()
                .map(|topic| support::query_topic(topic, 0))
                .collect(),
            limit: 10,
        })
        .await?;
    for include_full_envelope in [false, true] {
        server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: topics.clone(),
                include_full_envelope,
            })
            .await?;
    }
    server
        .query()
        .get(api::GetRequest {
            sequence_id: metas[0].cursor.as_ref().unwrap().sequence_id,
        })
        .await?;
    server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest {
            requests: vec![api::get_inbox_ids_request::Request {
                identifier: fixture.added_identifier.to_string(),
                identifier_kind: xmtp_proto::xmtp::identity::associations::IdentifierKind::Passkey
                    as i32,
            }],
        })
        .await?;
    let _ = server
        .identity()
        .verify_smart_contract_wallet_signatures(api::VerifySmartContractWalletSignaturesRequest {
            signatures: vec![
                api::verify_smart_contract_wallet_signatures_request::Signature {
                    account_id: "eip155:1:0x1111111111111111111111111111111111111111".into(),
                    hash: vec![1; 32],
                    signature: vec![1],
                    block_number: Some(1),
                },
            ],
        })
        .await;
    drop(stream.input);
    assert!(terminal(&mut stream.output).await?.is_none());
    server.stop().await?;
    logging.disable_telemetry()?;
    logging.disable_file()?;
    let rendered = xmtp_common::http::client()?
        .get(format!("http://{address}/metrics"))
        .send()
        .await?
        .text()
        .await?;
    for spec in crate::telemetry::CATALOGUE {
        if rendered.lines().any(|line| line.starts_with(spec.name)) {
            assert!(rendered.contains(&format!("# HELP {} {}", spec.name, spec.help)));
            assert!(rendered.contains(&format!("# TYPE {} ", spec.name)));
        }
    }
    let spans = collector.take_spans();
    assert!(!spans.is_empty());
    assert!(collector.take_logs().is_empty());
    let span_text = format!("{spans:?}");
    let log_text = logs.read();
    assert!(log_text.contains("gRPC request completed"));
    let inbox_bytes = hex::decode(&inbox)?;
    for forbidden in [
        hex::encode(&topic),
        format!("{topic:?}"),
        inbox.clone(),
        format!("{inbox_bytes:?}"),
    ] {
        for text in [&rendered, &span_text, &log_text] {
            assert!(
                !text.contains(&forbidden),
                "telemetry contains private routing data"
            );
        }
    }
    let mut names = std::collections::HashSet::new();
    let mut poll_seen = false;
    for resource in &spans {
        let attributes = &resource.resource.as_ref()?.attributes;
        assert_eq!(
            string_attribute(attributes, "service.name"),
            Some("xmtp-backend")
        );
        assert_eq!(
            string_attribute(attributes, "service.version"),
            Some(env!("CARGO_PKG_VERSION"))
        );
        for scope in &resource.scope_spans {
            for span in &scope.spans {
                names.insert(span.name.as_str());
                if span.name == "tailer.poll" {
                    assert!(
                        span.attributes
                            .iter()
                            .any(|attribute| attribute.key == "rows")
                    );
                    assert!(
                        span.attributes
                            .iter()
                            .any(|attribute| attribute.key == "gaps")
                    );
                    poll_seen = true;
                }
            }
        }
    }
    assert!(poll_seen);
    for name in [
        "db.commit_publish",
        "db.find_duplicates",
        "db.history",
        "db.query",
        "db.newest_envelopes",
        "db.newest_metadata",
        "db.get",
        "db.inbox_ids",
        "db.advance",
        "db.forward",
        "db.payloads",
        "publish.parse_publish",
        "publish.validate_publish",
        "publish.locks",
        "stream.update",
        "stream.fetch",
        "tailer.bootstrap",
        "scw.verify",
    ] {
        assert!(names.contains(name), "missing operation {name}");
    }
    let completions: Vec<serde_json::Value> = log_text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["fields"]["message"] == "gRPC request completed")
        .collect();
    assert!(!completions.is_empty());
    assert!(completions.iter().all(|event| {
        event["fields"]["trace_id"]
            .as_str()
            .is_some_and(|trace| trace.len() == 32)
    }));
    assert!(
        support::metrics::value(
            &metrics,
            "xmtp_operation_duration_seconds_count",
            &[("operation", "db.commit_publish")]
        ) > 0.0
    );
}
