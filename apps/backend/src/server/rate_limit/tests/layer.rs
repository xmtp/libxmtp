//! Where the layer runs, what it charges, and the rejection it sends.
use super::super::charges::{CHARGES, Charges};
use super::*;
use crate::{
    api,
    test_support::{
        TestServer,
        auth::{TestKey, mint, valid_claims},
        native::{envelope, raw_unary},
    },
};
use std::{convert::Infallible, net::Ipv4Addr};
use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};
use tower::{ServiceExt, service_fn};

const LOCALHOST: CallerKey = CallerKey::Ipv4([127, 0, 0, 1]);

async fn lookup(server: &TestServer) -> Result<(), Status> {
    server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest::default())
        .await
        .map(drop)
}

// verifies: RATE-002
#[xmtp_common::test(unwrap_try = true)]
async fn a_request_over_tcp_is_charged_to_its_peer_address() {
    // Only the peer's own key has a limit small enough to trip.
    let server = TestServer::new(|config| {
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             [[overrides]]\nip = \"127.0.0.1\"\n\
             [overrides.limits]\nrequest = [{ count = 1, window_seconds = 3600 }]",
        )
    })
    .await?;
    lookup(&server).await?;
    assert!(rate_limit_delay(&lookup(&server).await, Category::Request).is_some());
    server.stop().await?;
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn auth_rejected_exempt_and_unknown_requests_spend_nothing() {
    let key = TestKey::es256();
    let server = TestServer::new(|config| {
        config.auth = Some(key.auth_config());
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             [categories.request]\nip = [{ count = 1, window_seconds = 3600 }]",
        );
    })
    .await?;
    // A token without `sub` names no caller, so it is charged to the address
    // every other request here comes from.
    let anonymous = mint(&valid_claims(), &key);
    for _ in 0..3 {
        let rejected = server
            .identity()
            .get_inbox_ids(authorized(api::GetInboxIdsRequest::default(), "invalid")?)
            .await
            .unwrap_err();
        assert_eq!(rejected.code(), Code::Unauthenticated);
        HealthClient::new(server.channel.clone())
            .check(HealthCheckRequest::default())
            .await?;
        server
            .configuration()
            .get_configuration(api::GetConfigurationRequest {})
            .await?;
        let unknown = raw_unary(
            &server,
            "/xmtp.backend.v1.QueryService/QueryOldest",
            &[("authorization", &format!("Bearer {anonymous}"))],
            crate::test_support::native::framed(&[]),
            true,
        )
        .await?;
        assert_eq!(unknown.code(), Code::Unimplemented);
    }
    let anonymous_lookup = || async {
        server
            .identity()
            .get_inbox_ids(authorized(api::GetInboxIdsRequest::default(), &anonymous).unwrap())
            .await
            .map(drop)
    };
    // The address kept its whole allowance.
    anonymous_lookup().await?;
    assert!(rate_limit_delay(&anonymous_lookup().await, Category::Request).is_some());
    server.stop().await?;
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_request_refused_while_draining_spends_nothing() {
    let mut server = TestServer::new(|config| {
        config.server.max_drain_duration_ms = 5_000;
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             [categories.request]\nip = [{ count = 2, window_seconds = 3600 }]",
        );
    })
    .await?;
    let limiter = server.backend.rate_limiter.clone().ok_or("limiter")?;
    // A publish held by a table lock keeps the instance draining.
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let mut publisher = server.publisher();
    let publish = tokio::spawn(async move {
        publisher
            .publish(api::PublishRequest {
                envelopes: vec![envelope(98, 1)],
            })
            .await
    });
    // A waiting lock in this test's database, not any other on the server.
    crate::test_support::locks::wait_for_lock(&server.backend.store.primary, "relation").await?;
    server.shutdown();
    // Once admission refuses a stream, HTTP/2 can refuse the next before it
    // reaches any layer, so one refusal by admission is what this can see.
    let mut refused = false;
    for _ in 0..50 {
        let status = server
            .query()
            .query_newest(api::QueryNewestRequest::default())
            .await
            .unwrap_err();
        if status.code() == Code::Unavailable && status.message() == "server is shutting down" {
            refused = true;
            break;
        }
    }
    assert!(refused, "admission refused a request while draining");
    // Only the held publish spent a unit.
    let caller = probe(&limiter, LOCALHOST);
    caller.charge(Category::Request, 1).await?;
    assert!(caller.charge(Category::Request, 1).await.is_err());
    blocker.commit().await?;
    publish.await??;
    server.stop().await?;
}

/// A request through the layer alone to a handler that reports whether it
/// was given a charger.
async fn through_layer(limiter: &Arc<Limiter>, path: &str) -> TestResult<bool> {
    let service = RateLimitLayer {
        limiter: limiter.clone(),
        identifier: Arc::from("org.xmtp.test"),
    }
    .layer(service_fn(|request: Request<Body>| async move {
        let charged = request.extensions().get::<Charger>().is_some();
        Ok::<_, Infallible>(
            Response::builder()
                .header("charged", charged.to_string())
                .body(Body::empty())
                .unwrap(),
        )
    }));
    let response = service
        .oneshot(Request::post(path).body(Body::empty())?)
        .await?;
    Ok(response.headers()["charged"] == "true")
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn the_layer_spends_one_request_unit_and_nothing_else() {
    let clock = ManualClock::new();
    let every_category = Category::ALL
        .iter()
        .map(|category| {
            format!(
                "[categories.{}]\ndefault = [{{ count = 2, window_seconds = 3600 }}]\n",
                category.as_str()
            )
        })
        .collect::<String>();
    // A request without a peer address is charged to the unspecified address.
    let caller = CallerKey::from_ip(Ipv4Addr::UNSPECIFIED.into());
    let paths = CHARGES
        .iter()
        .map(|(path, charges)| (*path, matches!(charges, Charges::Charged(_))))
        .chain([("/xmtp.backend.v1.QueryService/QueryOldest", false)]);
    for (path, charged) in paths {
        let limiter = limiter(&format!("max_memory_mib = 1\n{every_category}"), &clock);
        assert_eq!(through_layer(&limiter, path).await?, charged, "{path}");
        let probe = probe(&limiter, caller.clone());
        for category in Category::ALL {
            let left = if charged && category == Category::Request {
                1
            } else {
                2
            };
            probe.charge(category, left).await?;
            assert!(
                probe.charge(category, 1).await.is_err(),
                "{path} spent more {} than it should",
                category.as_str()
            );
        }
    }
}

// verifies: RATE-012
#[xmtp_common::test(unwrap_try = true)]
async fn disabled_rate_limits_build_no_table_and_deny_nothing() {
    let limits = "[categories.request]\ndefault = [{ count = 1, window_seconds = 3600 }]";
    let disabled = TestServer::new(|config| {
        config.rate_limits = rate_limits(&format!("enabled = false\n{limits}"));
    })
    .await?;
    assert!(disabled.backend.rate_limiter.is_none());
    for _ in 0..3 {
        lookup(&disabled).await?;
    }
    disabled.stop().await?;
    let enabled = TestServer::new(|config| {
        config.rate_limits = rate_limits(&format!("max_memory_mib = 1\n{limits}"));
    })
    .await?;
    assert!(enabled.backend.rate_limiter.is_some());
    lookup(&enabled).await?;
    assert!(rate_limit_delay(&lookup(&enabled).await, Category::Request).is_some());
    enabled.stop().await?;
}

// verifies: RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn a_denial_carries_its_category_and_retry_delay_and_no_caller() {
    let clock = ManualClock::new();
    let server = TestServer::with_rate_limit_clock(
        |config| {
            config.rate_limits = rate_limits(
                "max_memory_mib = 1\n\
                 [categories.request]\ndefault = [{ count = 1, window_seconds = 60 }]",
            )
        },
        clock,
    )
    .await?;
    lookup(&server).await?;
    let status = lookup(&server).await.unwrap_err();
    assert_eq!(status.code(), Code::ResourceExhausted);
    let details = status.get_error_details();
    let info = details.error_info().ok_or("ErrorInfo")?;
    assert_eq!(info.reason, "RATE_LIMITED");
    assert_eq!(info.domain, "xmtp.org");
    assert_eq!(
        info.metadata,
        [("category".to_owned(), "request".to_owned())].into()
    );
    // One unit per 60 s, spent at the same instant: the next is earned at 60 s.
    assert_eq!(
        details.retry_info().ok_or("RetryInfo")?.retry_delay,
        Some(Duration::from_secs(60))
    );
    assert!(status.message().contains("request"));
    assert!(!status.message().contains("127.0.0.1"));
    server.stop().await?;
}

// verifies: RATE-014, API-304, PUSH-264, ATCH-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_denied_request_spend_decides_the_status_over_every_later_check() {
    use crate::test_support::native::framed;
    let server = TestServer::new(|config| {
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             [categories.request]\nip = [{ count = 1, window_seconds = 3600 }]",
        )
    })
    .await?;
    lookup(&server).await?;
    const PUBLISH: &str = "/xmtp.backend.v1.PublishService/Publish";
    let refused = [
        // A body that does not decode.
        raw_unary(&server, PUBLISH, &[], framed(&[0xff, 0xff, 0xff]), true).await?,
        // More envelopes than the count gate admits.
        raw_unary(
            &server,
            PUBLISH,
            &[],
            framed(&[0x0a, 0x00].repeat(xmtp_configuration::MAX_PUBLISH_ENVELOPES + 1)),
            true,
        )
        .await?,
        // More subscription entries than a batch admits.
        server
            .notifications()
            .update_subscriptions(api::UpdateSubscriptionsRequest {
                recipient_id: vec![1; 32],
                recipient_secret: vec![2; 32],
                adds: vec![],
                removes: vec![vec![1]; xmtp_configuration::NOTIFICATION_BATCH_TOPICS + 1],
            })
            .await
            .unwrap_err(),
        // An upload with no storage target and a malformed digest.
        server
            .attachments()
            .create_upload(api::CreateUploadRequest::default())
            .await
            .unwrap_err(),
    ];
    for status in refused {
        assert!(
            rate_limit_delay(&Err::<(), _>(status.clone()), Category::Request).is_some(),
            "{status:?}"
        );
    }
    server.stop().await?;
}
