//! What a publish is charged, and that a denied step stores nothing.

use super::signatures::rejection;
use super::transport::{PUBLISH, empty_envelopes};
use super::*;
use crate::{
    config::{CallerKey, Category},
    server::rate_limit::Charger,
    test_support::{native::envelope, rate_limit_delay, rate_limits},
};
use support::native::raw_unary;
use xmtp_mls_validation::test_utils::scw_create_inbox_update;

/// A server that charges each forwarded address as its own caller and
/// admits at most `max_publish_topics` topics in a publish.
async fn server(limits: &str, max_publish_topics: usize) -> support::TestResult<TestServer> {
    TestServer::new(|config| {
        config.limits.max_publish_topics = max_publish_topics;
        config.rate_limits = rate_limits(&format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n{limits}"
        ));
    })
    .await
}

/// A charger for the caller at `address`, to read what it has left.
fn caller(server: &TestServer, address: &str) -> Charger {
    let limiter = server
        .backend
        .rate_limiter
        .as_ref()
        .expect("rate limits on");
    Charger::for_test(limiter, CallerKey::from_ip(address.parse().unwrap()))
}

async fn publish_from(
    server: &TestServer,
    address: &str,
    envelopes: Vec<api::ClientEnvelope>,
) -> Result<api::PublishResponse, tonic::Status> {
    let mut request = tonic::Request::new(api::PublishRequest { envelopes });
    request
        .metadata_mut()
        .insert("x-forwarded-for", address.parse().unwrap());
    server
        .publisher()
        .publish(request)
        .await
        .map(tonic::Response::into_inner)
}

/// Spend exactly `left` units of `category`, and show none is left after.
async fn assert_left(caller: &Charger, category: Category, left: usize) {
    caller.charge(category, left).await.unwrap();
    assert!(
        caller.charge(category, 1).await.is_err(),
        "more than {left} {} left",
        category.as_str()
    );
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_is_charged_by_the_request_as_received() {
    let ten = "[{ count = 10, window_seconds = 3600 }]";
    let server = server(
        &format!(
            "[categories.publish]\nip = {ten}\n\
         [categories.identity_update]\nip = {ten}\n\
         [categories.chain_verify]\nip = {ten}"
        ),
        10,
    )
    .await?;
    const ADDRESS: &str = "203.0.113.1";
    // One identity update with one ERC-6492 signature, and three welcomes of
    // which two are the same. No chain is configured, so the update fails
    // validation after it is charged.
    let _ = publish_from(
        &server,
        ADDRESS,
        vec![
            identity_envelope(scw_create_inbox_update()),
            envelope(1, 1),
            envelope(1, 1),
            envelope(2, 1),
        ],
    )
    .await;
    // A publish with no envelopes costs one.
    publish_from(&server, ADDRESS, vec![]).await?;
    let caller = caller(&server, ADDRESS);
    assert_left(&caller, Category::IdentityUpdate, 9).await;
    assert_left(&caller, Category::ChainVerify, 9).await;
    assert_left(&caller, Category::Publish, 5).await;
    server.stop().await?;
}

// verifies: RATE-010
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_denied_by_a_charge_stores_nothing() {
    let fixture = identity_history_with_passkey().await;
    let server = server(
        &format!(
            "[categories.identity_update]\nip = [{{ count = {}, window_seconds = 3600 }}]\n\
         [categories.publish]\nip = [{{ count = 1, window_seconds = 3600 }}]\n\
         [[overrides]]\nip = \"203.0.113.1\"\n[overrides.limits]\npublish = []",
            fixture.history.len()
        ),
        10,
    )
    .await?;
    // Identity updates: the caller's whole history, then one update more.
    for update in &fixture.history {
        publish_from(
            &server,
            "203.0.113.1",
            vec![identity_envelope(update.clone())],
        )
        .await?;
    }
    let denied = publish_from(
        &server,
        "203.0.113.1",
        vec![identity_envelope(fixture.update.clone())],
    )
    .await;
    assert!(rate_limit_delay(&denied, Category::IdentityUpdate).is_some());
    let topic = xmtp_proto::types::TopicKind::IdentityUpdatesV1
        .create(hex::decode(&fixture.inbox_id)?)
        .to_vec();
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes WHERE topic = $1")
        .bind(&topic)
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(stored as usize, fixture.history.len());
    // Envelopes: one, then a second refused before it is stored.
    publish_from(&server, "203.0.113.2", vec![envelope(1, 1)]).await?;
    let denied = publish_from(&server, "203.0.113.2", vec![envelope(2, 1)]).await;
    assert!(rate_limit_delay(&denied, Category::Publish).is_some());
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes WHERE topic <> $1")
        .bind(&topic)
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(stored, 1);
    server.stop().await?;
}

// verifies: RATE-010, RATE-014
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_over_its_bounds_spends_only_its_request_unit() {
    let server = server(
        "[categories.request]\nip = [{ count = 3, window_seconds = 3600 }]\n\
         [categories.publish]\nip = [{ count = 1, window_seconds = 3600 }]",
        1,
    )
    .await?;
    // More envelopes than the count gate admits.
    let status = raw_unary(
        &server,
        PUBLISH,
        &[("x-forwarded-for", "203.0.113.1")],
        empty_envelopes(xmtp_configuration::MAX_PUBLISH_ENVELOPES + 1, &[]),
        true,
    )
    .await?;
    assert_eq!(status.code(), Code::InvalidArgument);
    // More topics than `limits.max_publish_topics`, checked by the handler.
    let status = publish_from(&server, "203.0.113.1", vec![envelope(1, 1), envelope(2, 1)])
        .await
        .unwrap_err();
    assert_eq!(rejection(status)?, (None, Reason::TooLarge));
    let caller = caller(&server, "203.0.113.1");
    assert_left(&caller, Category::Request, 1).await;
    assert_left(&caller, Category::Publish, 1).await;
    server.stop().await?;
}
