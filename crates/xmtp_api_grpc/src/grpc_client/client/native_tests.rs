use super::GrpcClient;
use xmtp_proto::types::AppVersion;

#[xmtp_common::test(unwrap_try = true)]
async fn every_transport_propagates_client_trace_context() {
    use tracing::Instrument as _;
    use xmtp_proto::api::Client as _;
    // The OTLP Export method accepts an empty protobuf message. Its unary
    // response is also a valid one-message server stream or bidi response.
    const METHOD: &str = "/opentelemetry.proto.collector.trace.v1.TraceService/Export";
    const PARENT: &str = "00-12345678901234567890123456789012-1234567890123456-01";
    const TRACE_ID: &str = "12345678901234567890123456789012";
    const STATE: &str = "vendor=value";
    const REQUESTS: usize = 3;
    let collector = xmtp_logging::test_logging::OtlpCollector::start().await?;
    let client = GrpcClient::create_with_version(
        collector.endpoint().parse()?,
        AppVersion::from("test/1.0.0"),
    )?;
    for enabled in [false, true] {
        let client = client.clone();
        let (_, spans) = tokio::task::spawn_blocking(move || {
            xmtp_logging::test_logging::with_trace_layer(enabled, || {
                tokio::runtime::Handle::current().block_on(async {
                    let parent = tracing::info_span!("parent");
                    let mut incoming = http::HeaderMap::new();
                    incoming.insert("traceparent", PARENT.parse().unwrap());
                    incoming.insert("tracestate", STATE.parse().unwrap());
                    assert_eq!(
                        xmtp_logging::propagation::set_parent(
                            &parent,
                            xmtp_logging::propagation::extract(&incoming).unwrap()
                        ),
                        enabled
                    );
                    async {
                        client
                            .request(
                                Default::default(),
                                METHOD.parse().unwrap(),
                                Default::default(),
                            )
                            .await
                            .unwrap();
                        client
                            .stream(
                                Default::default(),
                                METHOD.parse().unwrap(),
                                Default::default(),
                            )
                            .await
                            .unwrap();
                        client
                            .bidi_stream(
                                Default::default(),
                                METHOD.parse().unwrap(),
                                Box::pin(futures::stream::iter([prost::bytes::Bytes::new()])),
                            )
                            .await
                            .unwrap();
                    }
                    .instrument(parent)
                    .await;
                });
            })
        })
        .await?;
        let headers = collector.take_trace_headers();
        assert_eq!(headers.len(), REQUESTS);
        let mut span_ids = std::collections::HashSet::new();
        for headers in headers {
            assert_eq!(headers["x-app-version"], "test/1.0.0");
            assert_eq!(headers["x-libxmtp-version"], env!("CARGO_PKG_VERSION"));
            assert_eq!(headers.contains_key("traceparent"), enabled);
            if enabled {
                let traceparent = headers["traceparent"].to_str().unwrap();
                let mut parts = traceparent.split('-');
                assert_eq!(parts.next(), Some("00"));
                assert_eq!(parts.next(), Some(TRACE_ID));
                span_ids.insert(parts.next().unwrap().to_owned());
                assert_eq!(headers["tracestate"], STATE);
            } else {
                assert!(!headers.contains_key("tracestate"));
            }
        }
        if enabled {
            assert_eq!(span_ids.len(), REQUESTS);
            for operation in [
                "rpc.grpc.request",
                "rpc.grpc.stream",
                "rpc.grpc.bidi_stream",
            ] {
                let span = spans
                    .iter()
                    .find(|span| span.name == operation)
                    .expect("transport span exported under its operation name");
                assert_eq!(format!("{:?}", span.span_kind), "Client");
                assert!(span_ids.remove(&span.span_context.span_id().to_string()));
            }
            assert!(span_ids.is_empty());
        }
    }
}
