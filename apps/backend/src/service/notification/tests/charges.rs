//! The `push` spend: one unit per topic added, at admission check 2.

use super::*;
use crate::{
    config::{CallerKey, Category},
    server::rate_limit::Charger,
    test_support::{rate_limit_delay, rate_limits},
};

/// A server whose callers are their forwarded addresses, with 4 `push` units
/// each.
async fn server() -> crate::test_support::TestResult<TestServer> {
    TestServer::new(|config| {
        configured(config);
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n\
             [categories.push]\nip = [{ count = 4, window_seconds = 3600 }]",
        );
    })
    .await
}

fn from<T>(address: &str, message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    request
        .metadata_mut()
        .insert("x-forwarded-for", address.parse().unwrap());
    request
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

async fn assert_push_left(server: &TestServer, address: &str, left: usize) {
    let caller = caller(server, address);
    caller.charge(Category::Push, left).await.unwrap();
    assert!(caller.charge(Category::Push, 1).await.is_err());
}

fn topic(id: u8) -> Vec<u8> {
    TopicKind::GroupMessagesV1.create([id; 32]).to_vec()
}

// verifies: PUSH-253, RATE-010, RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn each_added_topic_costs_one_push_unit() {
    let server = server().await?;
    let mut client = server.notifications();
    client.register(from("203.0.113.1", registration())).await?;
    let adds = (1..=3).map(|id| subscription(id, 0)).collect();
    client
        .update_subscriptions(from("203.0.113.1", update(adds, vec![topic(8), topic(9)])))
        .await?;
    assert_push_left(&server, "203.0.113.1", 1).await;
    // Removes cost nothing.
    client
        .update_subscriptions(from(
            "203.0.113.2",
            update(vec![], vec![topic(8), topic(9)]),
        ))
        .await?;
    assert_push_left(&server, "203.0.113.2", 4).await;
    assert_count(&server, 3).await;
    server.stop().await?;
}

// verifies: PUSH-253, RATE-010, RATE-014
#[xmtp_common::test(unwrap_try = true)]
async fn the_push_spend_follows_checks_0_and_1_and_precedes_the_recipient_lookup() {
    let server = server().await?;
    let mut client = server.notifications();
    client.register(from("203.0.113.1", registration())).await?;
    client
        .update_subscriptions(from(
            "203.0.113.1",
            update(vec![subscription(1, 0)], vec![]),
        ))
        .await?;
    // Check 0, over the batch bound, and check 1, a malformed identity,
    // spend no `push` unit.
    let oversized = (0..=1000)
        .map(|id| {
            let mut add = subscription(0, 0);
            add.topic = TopicKind::GroupMessagesV1
                .create([(id as u128).to_be_bytes(), [0; 16]].concat())
                .to_vec();
            add
        })
        .collect();
    let refused = client
        .update_subscriptions(from("203.0.113.4", update(oversized, vec![])))
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::InvalidArgument);
    let mut malformed = update(vec![subscription(2, 0), subscription(3, 0)], vec![]);
    malformed.recipient_id = vec![11; 31];
    let refused = client
        .update_subscriptions(from("203.0.113.4", malformed))
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert_push_left(&server, "203.0.113.4", 4).await;
    // Out of `push` units: refused for the caller's allowance before the
    // recipient is read, whether or not it exists, and nothing is stored.
    caller(&server, "203.0.113.3")
        .charge(Category::Push, 4)
        .await?;
    let mut unknown = update(vec![subscription(2, 0)], vec![]);
    unknown.recipient_id = vec![33; 32];
    for request in [unknown, update(vec![subscription(2, 0)], vec![topic(1)])] {
        let denied = client
            .update_subscriptions(from("203.0.113.3", request))
            .await;
        assert!(rate_limit_delay(&denied, Category::Push).is_some());
    }
    assert_count(&server, 1).await;
    server.stop().await?;
}
