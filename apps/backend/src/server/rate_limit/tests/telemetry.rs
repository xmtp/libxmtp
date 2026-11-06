//! What a denial and an eviction record, and what they keep out.
use super::*;
use crate::{
    api,
    auth::{
        keys::{KeySet, inline},
        verify::Verifier,
    },
    server::{
        auth::AuthLayer,
        telemetry::{GrpcStatusLayer, GrpcTelemetryLayer},
    },
    test_support::{
        TestServer,
        auth::{TestKey, api_key, mint, valid_claims},
        metrics,
        native::envelope,
    },
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::convert::Infallible;
use tower::{ServiceExt, service_fn};
use tracing::instrument::WithSubscriber;

// verifies: OPS-017
#[xmtp_common::test(unwrap_try = true)]
async fn each_denial_and_each_eviction_is_counted_once() {
    let Some(metrics) = metrics::isolated(
        "server::rate_limit::tests::telemetry::each_denial_and_each_eviction_is_counted_once",
    ) else {
        return;
    };
    let server = TestServer::new(|config| {
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             [categories.request]\nip = [{ count = 2, window_seconds = 3600 }]\n\
             [categories.publish]\nip = [{ count = 1, window_seconds = 3600 }]",
        )
    })
    .await?;
    let publish = || {
        let mut client = server.publisher();
        async move {
            client
                .publish(api::PublishRequest {
                    envelopes: vec![envelope(1, 1)],
                })
                .await
        }
    };
    publish().await?;
    assert!(rate_limit_delay(&publish().await, Category::Publish).is_some());
    assert!(rate_limit_delay(&publish().await, Category::Request).is_some());
    let rejections = |category: &str| {
        metrics::value(
            &metrics,
            "xmtp_rate_limit_rejections_total",
            &[("category", category), ("auth_type", "ip")],
        )
    };
    assert_eq!(rejections("publish"), 1.0);
    assert_eq!(rejections("request"), 1.0);
    server.stop().await?;

    // A table of one set holds 8 callers. Each later caller evicts one that
    // has not earned back its spend.
    let clock = ManualClock::new();
    let limiter = BackendLimiter::new(MemoryLimiter::with_clock(0, clock), config(""));
    assert_eq!(limiter.limiter.slots(), 8);
    for caller in 0..20 {
        let key = CallerKey::ApiKey(format!("caller-{caller}"));
        let mut bytes = Vec::new();
        key.to_bytes(&mut bytes);
        assert_eq!(
            limiter.spend(&key, &bytes, Category::Request, 1).await,
            Decision::Allowed
        );
    }
    assert_eq!(limiter.limiter.evictions(), 12);
    assert_eq!(
        metrics::value(&metrics, "xmtp_rate_limit_evictions_total", &[]),
        12.0
    );
}

const SUBJECT: &str = "private-subject-sentinel";
const ISSUER: &str = "https://private-issuer-sentinel.example";
const CLIENT: &str = "203.0.113.77";
const PEER: &str = "127.0.0.1";

// verifies: OPS-014, RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn a_denial_logs_its_request_and_category_and_no_caller() {
    let key = TestKey::es256();
    let (api_key_value, mut api_keys) = api_key("operator");
    let mut auth = key.auth_config();
    auth.api_keys = std::mem::take(&mut api_keys.api_keys);
    let verifier = Arc::new(Verifier::new(Arc::new(KeySet::new(inline(&auth)?)), auth));
    let limiter = limiter(
        &format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"{PEER}/32\"]\n\
             [categories.request]\ndefault = [{{ count = 1, window_seconds = 3600 }}]"
        ),
        &ManualClock::new(),
    );
    let inner =
        service_fn(|_: Request<Body>| async { Ok::<_, Infallible>(Status::ok("").into_http()) });
    let service = GrpcTelemetryLayer::for_test(true).layer(
        GrpcStatusLayer.layer(
            AuthLayer::for_test(verifier).layer(
                RateLimitLayer {
                    limiter,
                    identifier: Arc::from("org.xmtp.test"),
                }
                .layer(inner),
            ),
        ),
    );
    let mut claims = valid_claims();
    claims["sub"] = json!(SUBJECT);
    claims["iss"] = json!(ISSUER);
    let user = mint(&claims, &key);
    // A token without `sub` is charged to the forwarded client address.
    let anonymous = mint(&valid_claims(), &key);
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Debug);
    let cases = [
        (user.as_str(), "jwt"),
        (anonymous.as_str(), "ip"),
        (api_key_value.as_str(), "api_key"),
    ];
    for (bearer, _) in cases {
        for _ in 0..2 {
            let mut request = Request::post("/xmtp.backend.v1.QueryService/Query")
                .header("content-type", "application/grpc")
                .header("authorization", format!("Bearer {bearer}"))
                .header("x-forwarded-for", CLIENT)
                .body(Body::empty())?;
            request.extensions_mut().insert(TcpConnectInfo {
                local_addr: None,
                remote_addr: Some(format!("{PEER}:40000").parse()?),
            });
            let response = service
                .clone()
                .oneshot(request)
                .with_subscriber(capture.dispatch())
                .await?;
            response.into_body().collect().await?;
        }
    }
    let output = capture.output();
    let events: Vec<Value> = output
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let denials: Vec<&Value> = events
        .iter()
        .filter(|event| event["message"] == "rate limit exceeded")
        .collect();
    assert_eq!(denials.len(), cases.len(), "one event per denial");
    // Each denial names the request its completion log names.
    let refused: Vec<&Value> = events
        .iter()
        .filter(|event| {
            event["message"] == "gRPC request completed"
                && event["grpc_code"] == "ResourceExhausted"
        })
        .map(|event| &event["request_id"])
        .collect();
    assert_eq!(refused.len(), cases.len());
    for ((denial, (_, auth_type)), request_id) in denials.iter().zip(cases).zip(refused) {
        assert_eq!(denial["level"], "DEBUG");
        assert!(
            denial["request_id"]
                .as_str()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        );
        assert_eq!(&denial["request_id"], request_id);
        assert_eq!(denial["identifier"], "org.xmtp.test");
        assert_eq!(denial["category"], "request");
        assert_eq!(denial["auth_type"], auth_type);
    }
    // Every request was logged at completion; the denials name none of the
    // caller's values, and neither does anything else.
    assert!(output.contains("gRPC request completed"));
    for forbidden in [SUBJECT, ISSUER, CLIENT, PEER] {
        assert!(!output.contains(forbidden), "{forbidden} reached a log");
    }
}
