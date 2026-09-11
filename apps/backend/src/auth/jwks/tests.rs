use super::*;
use crate::{
    auth::verify::Verifier,
    server,
    test_support::{
        TestServer,
        auth::{JwksResponse, JwksServer, TestKey, mint, valid_claims},
        metrics,
    },
};
use serde_json::json;
use std::sync::Arc;

#[xmtp_common::test(unwrap_try = true)]
async fn startup_waits_between_two_failures_then_loads_keys() {
    let key = TestKey::es256();
    let jwks = JwksServer::start(vec![
        JwksResponse::error(),
        JwksResponse::error(),
        JwksResponse::keys(&[key]),
    ])
    .await;
    let server = TestServer::new(|config| config.auth = Some(jwks.config())).await?;
    let requests = jwks.requests();
    assert_eq!(requests.len(), JWKS_STARTUP_ATTEMPTS);
    for pair in requests.windows(2) {
        assert!(pair[1].duration_since(pair[0]) >= JWKS_STARTUP_RETRY_DELAY);
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn startup_exhaustion_and_empty_key_sets_return_host_only_errors() {
    for reply in [
        JwksResponse::error(),
        JwksResponse::json(json!({"keys": []})),
    ] {
        let jwks = JwksServer::start(vec![reply]).await;
        let result = TestServer::new(|config| config.auth = Some(jwks.config())).await;
        let error = result.err().expect("startup must fail").to_string();
        assert!(error.contains("127.0.0.1"));
        assert!(!error.contains("private-response-sentinel"));
        assert!(!error.contains("/keys"));
        assert_eq!(jwks.requests().len(), JWKS_STARTUP_ATTEMPTS);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn fetch_rejects_redirects_oversized_streams_parse_errors_and_plain_remote_http() {
    xmtp_cryptography::install_crypto_provider();
    let target = JwksServer::start(vec![JwksResponse::keys(&[TestKey::es256()])]).await;
    for reply in [
        JwksResponse::redirect(target.url.clone()),
        JwksResponse::oversized(),
        JwksResponse {
            body: b"private-response-sentinel".to_vec(),
            ..JwksResponse::json(json!({}))
        },
        JwksResponse::json(json!({"keys": [{"kty": "oct", "alg": "HS256"}]})),
    ] {
        let jwks = JwksServer::start(vec![reply]).await;
        let source = JwksSource::new(&jwks.url, jwks.config())?;
        let error = source
            .fetch()
            .await
            .err()
            .expect("failed fetch")
            .to_string();
        assert!(error.contains("127.0.0.1"));
        assert!(!error.contains("private-response-sentinel"));
    }
    assert!(
        target.requests().is_empty(),
        "redirect must not reach its destination"
    );
    assert!(JwksSource::new("http://issuer.example/keys", AuthConfig::default()).is_err());
}

#[xmtp_common::test(unwrap_try = true)]
/// A broken or hostile endpoint must not be able to drive the log volume. Each
/// unusable entry costs one warning, so the number examined is bounded too.
#[xmtp_common::test(unwrap_try = true)]
async fn a_document_of_unusable_entries_bounds_the_warnings() {
    use tracing::instrument::WithSubscriber;
    xmtp_cryptography::install_crypto_provider();
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Warn);
    let entries: Vec<_> = (0..super::MAX_JWKS_ENTRIES * 4)
        .map(|index| json!({"kid": index.to_string(), "alg": "HS256", "kty": "oct"}))
        .collect();
    let jwks = JwksServer::start(vec![JwksResponse::json(json!({"keys": entries}))]).await;
    let source = JwksSource::new(&jwks.url, jwks.config())?;
    // Zero usable keys is a failed fetch, which is the point: it must fail
    // without logging one line per entry.
    assert!(
        source
            .fetch()
            .with_subscriber(capture.dispatch())
            .await
            .is_err()
    );
    let warnings = capture.output().lines().count();
    assert!(
        warnings <= super::MAX_JWKS_ENTRIES + 2,
        "{warnings} warnings for {} entries",
        super::MAX_JWKS_ENTRIES * 4
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn fetch_keeps_first_64_usable_keys_and_skips_unsupported_entries() {
    xmtp_cryptography::install_crypto_provider();
    let key = TestKey::es256();
    let mut entries = vec![json!({"kid": "skip", "alg": "HS256", "kty": "oct"})];
    for index in 0..MAX_JWKS_KEYS + 2 {
        let mut entry = key.jwk.clone();
        entry["kid"] = json!(index.to_string());
        entries.push(entry);
    }
    let jwks = JwksServer::start(vec![JwksResponse::json(json!({"keys": entries}))]).await;
    let source = JwksSource::new(&jwks.url, jwks.config())?;
    let loaded = source.fetch().await?;
    assert_eq!(loaded.len(), MAX_JWKS_KEYS);
    assert_eq!(loaded.first().unwrap().kid.as_deref(), Some("0"));
    assert_eq!(loaded.last().unwrap().kid.as_deref(), Some("63"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn refresh_swaps_success_keeps_failure_and_never_fetches_for_unknown_ids() {
    let Some(metrics) = metrics::isolated(
        "auth::jwks::tests::refresh_swaps_success_keeps_failure_and_never_fetches_for_unknown_ids",
    ) else {
        return;
    };
    xmtp_cryptography::install_crypto_provider();
    let old = TestKey::es256();
    let new = TestKey::eddsa();
    let jwks = JwksServer::start(vec![
        JwksResponse::keys(&[old]),
        JwksResponse::keys(std::slice::from_ref(&new)),
        JwksResponse::error(),
    ])
    .await;
    let mut config = jwks.config();
    config.jwks_refresh_seconds = 1;
    config.jwks_max_stale_seconds = 12;
    let source = JwksSource::new(&jwks.url, config.clone())?;
    let keys = Arc::new(KeySet::new(source.startup().await?));
    let refresh_keys = keys.clone();
    let task = tokio::spawn(async move {
        source.refresh(&refresh_keys, Instant::now()).await;
    });
    xmtp_common::wait_for_eq(
        || async {
            metrics::value(
                &metrics,
                "xmtp_auth_jwks_refresh_total",
                &[("result", "error")],
            ) >= 1.0
        },
        true,
    )
    .await?;
    assert_eq!(keys.0.load().len(), 1);
    assert_eq!(keys.0.load()[0].kid.as_deref(), Some(new.kid.as_str()));
    assert_eq!(
        metrics::value(
            &metrics,
            "xmtp_auth_jwks_refresh_total",
            &[("result", "ok")]
        ),
        1.0
    );
    assert_eq!(metrics::value(&metrics, "xmtp_auth_keys", &[]), 1.0);
    task.abort();
    let _ = task.await;
    let requests = jwks.requests().len();
    let verifier = Verifier::new(keys, config);
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", mint(&valid_claims(), &TestKey::eddsa())).parse()?,
    );
    assert!(verifier.verify(&headers).is_err());
    assert_eq!(jwks.requests().len(), requests);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_fetch_in_progress_cannot_extend_the_monotonic_stale_deadline() {
    use tracing::instrument::WithSubscriber;
    xmtp_cryptography::install_crypto_provider();
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Debug);
    let mut reply = JwksResponse::keys(&[TestKey::es256()]);
    reply.delay = Duration::from_secs(2);
    let jwks = JwksServer::start(vec![reply]).await;
    let mut config = jwks.config();
    config.jwks_refresh_seconds = 1;
    config.jwks_max_stale_seconds = 12;
    let source = JwksSource::new(&jwks.url, config)?;
    let keys = KeySet::new(super::super::keys::inline(&TestKey::es256().auth_config())?);
    // The loaded snapshot is close to its deadline before serving begins.
    let start = Instant::now();
    source
        .refresh(&keys, start - Duration::from_millis(10_500))
        .with_subscriber(capture.dispatch())
        .await;
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(jwks.requests().len(), 1);
    let errors: Vec<serde_json::Value> = capture
        .output()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|event: &serde_json::Value| event["level"] == "ERROR")
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(capture.output().contains("127.0.0.1"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn stale_keys_use_the_server_drain_and_return_a_typed_error() {
    use crate::{
        api,
        test_support::{native::envelope, query_topic, topic},
    };
    use tonic_health::pb::{
        HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
    };
    let key = TestKey::es256();
    let jwks = JwksServer::start(vec![
        JwksResponse::keys(std::slice::from_ref(&key)),
        JwksResponse::error(),
    ])
    .await;
    let mut config = jwks.config();
    config.jwks_refresh_seconds = 1;
    config.jwks_max_stale_seconds = 12;
    let mut server = TestServer::new(|defaults| {
        defaults.auth = Some(config);
        defaults.server.max_drain_duration_ms = 100;
        defaults.database.max_statement_timeout_ms = 20_000;
        defaults.publishing.max_publish_duration_ms = 30_000;
    })
    .await?;
    let mut health = HealthClient::new(server.channel.clone())
        .watch(HealthCheckRequest {
            service: String::new(),
        })
        .await?
        .into_inner();
    assert_eq!(
        health.message().await?.unwrap().status(),
        ServingStatus::Serving
    );
    let token = format!("Bearer {}", mint(&valid_claims(), &key));
    let mut request = tonic::Request::new(api::SubscribeStaticRequest {
        topics: vec![query_topic(
            topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[91; 32]),
            0,
        )],
    });
    request
        .metadata_mut()
        .insert("authorization", token.parse()?);
    let mut stream =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone())
            .subscribe_static(request)
            .await?
            .into_inner();
    assert!(matches!(
        stream.message().await?.unwrap().response,
        Some(api::subscribe_static_response::Response::Started(_))
    ));
    xmtp_common::wait_for_eq(|| async { jwks.requests().len() >= 2 }, true).await?;
    let mut request = tonic::Request::new(api::GetRequest { sequence_id: 1 });
    request
        .metadata_mut()
        .insert("authorization", token.parse()?);
    assert_eq!(
        server.query().get(request).await.unwrap_err().code(),
        tonic::Code::NotFound
    );
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let mut request = tonic::Request::new(api::PublishRequest {
        envelopes: vec![envelope(91, 1)],
    });
    request
        .metadata_mut()
        .insert("authorization", token.parse()?);
    let mut publisher = server.publisher();
    let publish = tokio::spawn(async move { publisher.publish(request).await });
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM pg_locks WHERE database = (SELECT oid FROM pg_database WHERE datname = current_database()) AND locktype = 'relation' AND NOT granted)")
            .fetch_one(&server.backend.store.primary).await.unwrap()
    }, true).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(15), health.message())
            .await??
            .unwrap()
            .status(),
        ServingStatus::NotServing
    );
    let draining = Instant::now();
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(1), stream.message())
            .await?
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    let error = xmtp_common::time::timeout(Duration::from_secs(1), server.wait_stopped())
        .await?
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<server::ServeError>(),
        Some(server::ServeError::JwksStale)
    ));
    assert!(draining.elapsed() < Duration::from_secs(1));
    assert!(
        xmtp_common::time::timeout(Duration::from_secs(1), publish)
            .await??
            .is_err()
    );
    blocker.commit().await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 0);
    drop((health, stream));
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_successful_refresh_moves_the_stale_deadline() {
    xmtp_cryptography::install_crypto_provider();
    let key = TestKey::eddsa();
    let jwks = JwksServer::start(vec![
        JwksResponse::keys(std::slice::from_ref(&key)),
        JwksResponse::error(),
    ])
    .await;
    let mut config = jwks.config();
    config.jwks_refresh_seconds = 1;
    config.jwks_max_stale_seconds = 12;
    let source = JwksSource::new(&jwks.url, config)?;
    let keys = KeySet::new(super::super::keys::inline(&TestKey::es256().auth_config())?);
    let result = xmtp_common::time::timeout(
        Duration::from_secs(3),
        source.refresh(&keys, Instant::now() - Duration::from_millis(9500)),
    )
    .await;
    assert!(
        result.is_err(),
        "a fresh key set must outlive the old deadline"
    );
    assert_eq!(keys.0.load()[0].kid.as_deref(), Some(key.kid.as_str()));
    assert!(jwks.requests().len() >= 2);
}
