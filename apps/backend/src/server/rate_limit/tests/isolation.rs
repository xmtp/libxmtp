//! One caller's denial leaves others admitted, and ends at its retry delay.
use super::*;
use crate::{
    api,
    config::auth::AuthConfig,
    test_support::{TestServer, auth::api_key_value, native::envelope},
};

/// A server whose callers are the API keys named here, on `clock`.
async fn server(
    names: &[&str],
    limits: &str,
    clock: &ManualClock,
) -> TestResult<(TestServer, Vec<String>)> {
    let values: Vec<String> = names.iter().map(|_| api_key_value()).collect();
    let mut auth = AuthConfig::default();
    auth.enabled = Some(true);
    auth.api_keys = names
        .iter()
        .map(|name| name.to_string())
        .zip(values.iter().cloned())
        .collect();
    let server = TestServer::with_rate_limit_clock(
        |config| {
            config.auth = Some(auth);
            config.rate_limits = rate_limits(&format!("max_memory_mib = 1\n{limits}"));
        },
        clock.clone(),
    )
    .await?;
    Ok((server, values))
}

// verifies: RATE-001, RATE-022, RATE-024
#[xmtp_common::test(unwrap_try = true)]
async fn a_request_denial_holds_only_its_caller_until_the_delay() {
    let clock = ManualClock::new();
    let (server, keys) = server(
        &["alice", "bob"],
        "[categories.request]\napi_key = [{ count = 2, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let lookup = |key: &str| {
        let request = authorized(api::GetInboxIdsRequest::default(), key).unwrap();
        let mut client = server.identity();
        async move { client.get_inbox_ids(request).await.map(drop) }
    };
    let (alice, bob) = (&keys[0], &keys[1]);
    lookup(alice).await?;
    lookup(alice).await?;
    let denied = lookup(alice).await;
    let delay = rate_limit_delay(&denied, Category::Request).ok_or("request denial")?;
    assert_eq!(delay, Duration::from_secs(30));
    lookup(bob).await?;
    clock.advance(delay.as_nanos() as u64 - millis(1));
    assert!(rate_limit_delay(&lookup(alice).await, Category::Request).is_some());
    clock.advance(millis(1));
    lookup(alice).await?;
    server.stop().await?;
}

// verifies: RATE-010, RATE-022, RATE-024
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_denial_holds_only_its_caller_until_the_delay() {
    let clock = ManualClock::new();
    // Requests are unlimited for both callers, so only the handler's
    // `publish` spend can deny.
    let (server, keys) = server(
        &["carol", "dave"],
        "[categories.publish]\napi_key = [{ count = 2, window_seconds = 60 }]\n\
         [[overrides]]\napi_key = \"carol\"\n[overrides.limits]\nrequest = []\n\
         [[overrides]]\napi_key = \"dave\"\n[overrides.limits]\nrequest = []",
        &clock,
    )
    .await?;
    let publish = |key: &str, envelopes: Vec<api::ClientEnvelope>| {
        let request = authorized(api::PublishRequest { envelopes }, key).unwrap();
        let mut client = server.publisher();
        async move { client.publish(request).await.map(drop) }
    };
    let (carol, dave) = (&keys[0], &keys[1]);
    publish(carol, vec![envelope(1, 1), envelope(1, 2)]).await?;
    let retried = vec![envelope(2, 1), envelope(2, 2)];
    let denied = publish(carol, retried.clone()).await;
    let delay = rate_limit_delay(&denied, Category::Publish).ok_or("publish denial")?;
    assert_eq!(delay, Duration::from_secs(60));
    publish(dave, vec![envelope(3, 1), envelope(3, 2)]).await?;
    clock.advance(delay.as_nanos() as u64 - millis(1));
    assert!(rate_limit_delay(&publish(carol, retried.clone()).await, Category::Publish).is_some());
    clock.advance(millis(1));
    publish(carol, retried).await?;
    server.stop().await?;
}
