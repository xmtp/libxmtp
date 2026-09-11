use super::*;
use crate::{
    api,
    auth::verify::Rejection,
    server::telemetry::{GrpcStatusLayer, GrpcTelemetryLayer},
    test_support::{TestServer, grpc_web, metrics},
};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::json;

#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn every_rejection_is_counted_once_without_disclosing_token_data() {
    let Some((metrics, address)) = metrics::isolated_http(
        "server::auth::tests::redaction::every_rejection_is_counted_once_without_disclosing_token_data",
    ) else {
        return;
    };
    const SENTINEL: &str = "auth-private-sentinel";
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Debug);
    tracing::dispatcher::set_global_default(capture.dispatch())?;
    let key = TestKey::es256();
    let mut other = TestKey::es256();
    other.kid = SENTINEL.into();
    let mut config = key.auth_config();
    config.required_scopes = vec!["required".into()];
    config.audiences = Some(vec!["backend".into()]);
    config.issuers = Some(vec!["issuer".into()]);
    let server = TestServer::new(|defaults| defaults.auth = Some(config.clone())).await?;
    let verifier = verifier(config);
    // One direct layer call and both wire transports for three RPC paths.
    const REQUESTS_PER_CASE: usize = 7;
    let now = xmtp_common::time::now_secs();
    let claims = json!({"exp": now + 3600, "sub": SENTINEL, "scope": "required", "aud": "backend", "iss": "issuer"});
    let bearer =
        |claims: &serde_json::Value, key: &TestKey| format!("Bearer {}", mint(claims, key));
    let mut cases = vec![
        (None, Rejection::Missing),
        (Some(format!("Basic {SENTINEL}")), Rejection::Bearer),
        (Some(format!("Bearer {SENTINEL}")), Rejection::Malformed),
        (
            Some(format!(
                "Bearer {}",
                jsonwebtoken::encode(
                    &Header::new(Algorithm::HS256),
                    &claims,
                    &EncodingKey::from_secret(SENTINEL.as_bytes())
                )?
            )),
            Rejection::UnsupportedAlg,
        ),
        (Some(bearer(&claims, &other)), Rejection::Untrusted),
    ];
    for (field, value, reason) in [
        ("exp", json!(1), Rejection::Expired),
        ("nbf", json!(now + 3600), Rejection::NotYetValid),
        ("aud", json!(SENTINEL), Rejection::Audience),
        ("iss", json!(SENTINEL), Rejection::Issuer),
        ("scope", json!(SENTINEL), Rejection::Scope),
    ] {
        let mut claims = claims.clone();
        claims[field] = value;
        cases.push((Some(bearer(&claims, &key)), reason));
    }
    let mut expected = std::collections::BTreeMap::new();
    let mut forbidden = vec![SENTINEL.to_owned(), key.kid.clone(), other.kid.clone()];
    for (header, reason) in &cases {
        let inner = service_fn(|_: Request<Body>| async {
            Ok::<_, Infallible>(
                Response::builder()
                    .header("handler-reached", "true")
                    .body(Body::empty())
                    .unwrap(),
            )
        });
        let mut service = GrpcTelemetryLayer(true)
            .layer(GrpcStatusLayer.layer(AuthLayer(verifier.clone()).layer(inner)));
        let mut request = Request::post("/xmtp.backend.v1.QueryService/Query")
            .header("content-type", "application/grpc")
            .header("x-request-id", SENTINEL)
            .body(Body::empty())?;
        if let Some(header) = header {
            request
                .headers_mut()
                .insert("authorization", header.parse()?);
            forbidden.push(header.clone());
            forbidden.push(header.split_once(' ').unwrap().1.to_owned());
        }
        let response =
            tracing::dispatcher::with_default(&capture.dispatch(), || service.call(request))
                .await?;
        assert!(!response.headers().contains_key("handler-reached"));
        let status = tonic::Status::from_header_map(response.headers()).unwrap();
        assert_eq!(status.code(), reason.status().code());
        assert_eq!(status.message(), reason.status().message());
        assert!(!format!("{status:?}").contains(SENTINEL));
        response.into_body().collect().await?;
        for path in [
            "/xmtp.backend.v1.QueryService/Get",
            "/xmtp.backend.v1.SubscriptionService/Subscribe",
            "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        ] {
            let mut native = tonic::client::Grpc::new(server.channel.clone());
            native.ready().await?;
            let mut request = tonic::Request::new(api::GetRequest { sequence_id: 1 });
            request
                .metadata_mut()
                .insert("x-request-id", SENTINEL.parse()?);
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("x-request-id", SENTINEL.parse()?);
            if let Some(header) = header {
                request
                    .metadata_mut()
                    .insert("authorization", header.parse()?);
                headers.insert("authorization", header.parse()?);
            }
            // Auth rejects before protobuf decoding, including at stream open.
            let error = native
                .server_streaming(
                    request,
                    path.parse()?,
                    tonic_prost::ProstCodec::<api::GetRequest, api::ServerEnvelope>::default(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code(), reason.status().code());
            assert_eq!(error.message(), reason.status().message());
            let client = xmtp_common::http::client_builder()
                .default_headers(headers)
                .build()?;
            let response: crate::test_support::TestResult<
                grpc_web::WebStream<api::ServerEnvelope>,
            > = grpc_web::open(
                &client,
                &server.url,
                path,
                api::GetRequest { sequence_id: 1 },
            )
            .await;
            let error = response.err().expect("auth must reject the request");
            let error = error.downcast_ref::<tonic::Status>().unwrap();
            assert_eq!(error.code(), reason.status().code());
            assert_eq!(error.message(), reason.status().message());
        }
        *expected.entry(reason.label()).or_insert(0_usize) += REQUESTS_PER_CASE;
    }
    let logs = capture.output();
    assert_eq!(
        logs.lines()
            .filter(|line| line.contains("authentication rejected"))
            .count(),
        cases.len() * REQUESTS_PER_CASE
    );
    let client = xmtp_common::http::client()?;
    let output = client
        .get(format!("http://{address}/metrics"))
        .send()
        .await?
        .text()
        .await?;
    for secret in forbidden {
        assert!(!logs.contains(&secret), "secret in logs");
        assert!(!output.contains(&secret), "secret in metrics");
    }
    for (reason, count) in expected {
        assert_eq!(
            metrics::value(
                &metrics,
                "xmtp_auth_rejections_total",
                &[("reason", reason)]
            ),
            count as f64
        );
    }
    for line in output
        .lines()
        .filter(|line| line.starts_with("xmtp_auth_rejections_total{"))
    {
        let labels = line.split_once('{').unwrap().1.split_once('}').unwrap().0;
        assert!(labels.starts_with("reason="));
        assert!(!labels.contains(','));
    }
    assert_eq!(
        metrics::value(
            &metrics,
            "grpc_server_handled_total",
            &[("grpc_code", "Unauthenticated")]
        ),
        ((cases.len() - 1) * REQUESTS_PER_CASE) as f64
    );
    assert_eq!(
        metrics::value(
            &metrics,
            "grpc_server_handled_total",
            &[("grpc_code", "PermissionDenied")]
        ),
        REQUESTS_PER_CASE as f64
    );
    server.stop().await?;
}
