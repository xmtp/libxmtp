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
        RequestLoggerLayer(true).layer(service_fn(|request: Request<Body>| async move {
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
        RequestLoggerLayer(true).layer(service_fn(|request: Request<Body>| async move {
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
        RequestLoggerLayer(true).layer(service_fn(|request: Request<Body>| async move {
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
        RequestLoggerLayer(true).layer(service_fn(|request: Request<Body>| async move {
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
    let mut service = RequestLoggerLayer(true).layer(service_fn(|_: Request<Body>| async move {
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
            RequestLoggerLayer(enabled).layer(service_fn(|request: Request<Body>| async move {
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
    let mut service = RequestLoggerLayer(true).layer(tonic_web::GrpcWebLayer::new().layer(inner));
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
        let mut service = RequestLoggerLayer(enabled).layer(inner);
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
    let mut service = RequestLoggerLayer(true).layer(inner);
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
