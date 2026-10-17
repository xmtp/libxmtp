use super::*;
use crate::{
    config::{
        Config,
        push::{ApnsConfig, FcmConfig, HttpConfig},
    },
    test_support::{RunningServer, TestServer},
};
use sqlx::Row;
use tonic::Code;

mod transport;

#[xmtp_common::test(unwrap_try = true)]
async fn a_thousand_topic_batch_and_key_only_renewal_preserve_the_count() {
    const TOPICS: usize = 1000;
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    let adds: Vec<_> = (0..TOPICS)
        .map(|id| api::Subscription {
            topic: TopicKind::GroupMessagesV1
                .create((id as u128).to_be_bytes())
                .to_vec(),
            hmac_epoch_base: 10,
            hmac_keys: vec![vec![1; HMAC_KEY_BYTES]; MAX_HMAC_KEYS],
            include_commits: false,
        })
        .collect();
    assert_eq!(
        client
            .update_subscriptions(update(adds.clone(), vec![]))
            .await?
            .into_inner()
            .topic_count,
        TOPICS as u64
    );
    let renewal = adds
        .into_iter()
        .map(|mut add| {
            add.hmac_epoch_base += 1;
            add
        })
        .collect();
    assert_eq!(
        client
            .update_subscriptions(update(renewal, vec![]))
            .await?
            .into_inner()
            .topic_count,
        TOPICS as u64
    );
    assert_count(&server, TOPICS as i64).await;
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_first_registrations_cannot_replace_another_secret() {
    let server = TestServer::new(configured).await?;
    let mut left = server.notifications();
    let mut right = server.notifications();
    let first = registration();
    let mut second = first.clone();
    second.recipient_secret = vec![99; 32];
    let (a, b) = tokio::join!(left.register(first.clone()), right.register(second.clone()));
    assert_ne!(a.is_ok(), b.is_ok());
    let winner = if a.is_ok() { first } else { second };
    status(
        a.err().or(b.err()).unwrap(),
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    let stored = server
        .backend
        .store
        .load_recipient(&winner.recipient_id)
        .await?
        .unwrap();
    assert_eq!(
        stored.secret_hash,
        xmtp_common::sha256_array(&winner.recipient_secret)
    );
    assert_count(&server, 0).await;
    server.stop().await?;
}
mod validation;

fn configured(config: &mut Config) {
    config.push.apns = Some(ApnsConfig {
        key: Some(crate::test_support::auth::TestKey::es256().private_key),
        key_id: Some("key-id".into()),
        team_id: Some("team-id".into()),
        bundle_id: Some("bundle-id".into()),
        environment: Some("sandbox".into()),
    });
    config.push.fcm = Some(FcmConfig {
        service_account: Some(crate::push::channel::fcm::tests::service_account()),
    });
    config.push.http = Some(HttpConfig {
        allow_private_addresses: true,
        ..Default::default()
    });
}

fn registration() -> api::RegisterRequest {
    api::RegisterRequest {
        recipient_id: vec![11; 32],
        recipient_secret: vec![22; 32],
        delivery: Some(api::register_request::Delivery::Fcm(api::FcmDelivery {
            token: "device-token".into(),
        })),
    }
}

fn subscription(id: u8, keys: usize) -> api::Subscription {
    api::Subscription {
        topic: TopicKind::GroupMessagesV1.create([id; 16]).to_vec(),
        hmac_epoch_base: 7,
        hmac_keys: (0..keys)
            .map(|n| vec![n as u8 + 1; HMAC_KEY_BYTES])
            .collect(),
        include_commits: true,
    }
}

fn update(adds: Vec<api::Subscription>, removes: Vec<Vec<u8>>) -> api::UpdateSubscriptionsRequest {
    let register = registration();
    api::UpdateSubscriptionsRequest {
        recipient_id: register.recipient_id,
        recipient_secret: register.recipient_secret,
        adds,
        removes,
    }
}

fn unregister() -> api::UnregisterRequest {
    let request = registration();
    api::UnregisterRequest {
        recipient_id: request.recipient_id,
        recipient_secret: request.recipient_secret,
    }
}

fn status(error: Status, code: Code, message: &str) {
    assert_eq!(error.code(), code);
    assert_eq!(error.message(), message);
}

async fn assert_count(server: &RunningServer, expected: i64) {
    let actual: i64 =
        sqlx::query_scalar("SELECT count(*) FROM push_subscription WHERE recipient_id = $1")
            .bind(registration().recipient_id)
            .fetch_one(&server.backend.store.primary)
            .await
            .unwrap();
    let topic_count: i32 =
        sqlx::query_scalar("SELECT topic_count FROM push_recipient WHERE recipient_id = $1")
            .bind(registration().recipient_id)
            .fetch_one(&server.backend.store.primary)
            .await
            .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(i64::from(topic_count), expected);
}

// verifies: PUSH-254, PUSH-255
#[xmtp_common::test(unwrap_try = true)]
async fn registration_hashes_secret_and_renewal_preserves_subscriptions() {
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    let request = registration();
    let first = client.register(request.clone()).await?.into_inner();
    assert_eq!(first.topic_count, 0);
    assert_eq!(first.channel, api::Channel::Fcm as i32);
    let row = server
        .backend
        .store
        .load_recipient(&request.recipient_id)
        .await?
        .unwrap();
    assert_eq!(
        row.secret_hash,
        xmtp_common::sha256_array(&request.recipient_secret)
    );
    assert_ne!(row.secret_hash, request.recipient_secret);
    assert_ne!(row.delivery.as_bytes(), request.recipient_secret);
    assert!(row.signing_key.is_none());
    assert_eq!(
        first.expires_at_ns,
        server.backend.config.push.expires_at(row.renewed_ns)?
    );
    client
        .update_subscriptions(update(vec![subscription(1, 3)], vec![]))
        .await?;
    let before: i64 = sqlx::query_scalar("SELECT since_sequence_id FROM push_subscription")
        .fetch_one(&server.backend.store.primary)
        .await?;
    let mut changed = request.clone();
    changed.delivery = Some(api::register_request::Delivery::Apns(api::ApnsDelivery {
        token: "replacement-token".into(),
    }));
    let renewed = client.register(changed.clone()).await?.into_inner();
    assert_eq!(renewed.topic_count, 1);
    assert_eq!(renewed.channel, api::Channel::Apns as i32);
    assert!(renewed.expires_at_ns >= first.expires_at_ns);
    let row = server
        .backend
        .store
        .load_recipient(&request.recipient_id)
        .await?
        .unwrap();
    assert_eq!(row.delivery, "replacement-token");
    let after: i64 = sqlx::query_scalar("SELECT since_sequence_id FROM push_subscription")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(before, after);
    changed.recipient_secret = row.secret_hash;
    status(
        client.register(changed).await.unwrap_err(),
        Code::PermissionDenied,
        "recipient secret is not valid",
    );
    assert_eq!(
        server
            .backend
            .store
            .load_recipient(&request.recipient_id)
            .await?
            .unwrap()
            .delivery,
        "replacement-token"
    );
    assert_count(&server, 1).await;
    server.stop().await?;
}

// verifies: PUSH-216
#[xmtp_common::test(unwrap_try = true)]
async fn updates_keep_start_positions_and_round_trip_key_slots() {
    use xmtp_mls_validation::test_utils::inline_welcome_envelope;
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    let before = server
        .publish(vec![inline_welcome_envelope([44; 32])])
        .await?[0]
        .cursor
        .as_ref()
        .unwrap()
        .sequence_id as i64;
    // Establish the settled boundary independently of the background worker.
    sqlx::query("UPDATE allocation_boundary SET closed_sequence_id = $1")
        .bind(before)
        .execute(&server.backend.store.primary)
        .await?;
    let adds = vec![subscription(1, 0), subscription(2, 1), subscription(3, 3)];
    client
        .update_subscriptions(update(adds.clone(), vec![]))
        .await?;
    let rows = sqlx::query("SELECT topic, since_sequence_id, hmac_epoch_base, hmac_key_0, hmac_key_1, hmac_key_2, include_commits FROM push_subscription ORDER BY topic")
        .fetch_all(&server.backend.store.primary).await?;
    for (row, add) in rows.iter().zip(&adds) {
        assert_eq!(row.get::<i64, _>("since_sequence_id"), before);
        assert_eq!(
            row.get::<Option<i64>, _>("hmac_epoch_base"),
            (!add.hmac_keys.is_empty()).then_some(7)
        );
        for (index, column) in ["hmac_key_0", "hmac_key_1", "hmac_key_2"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                row.get::<Option<Vec<u8>>, _>(column),
                add.hmac_keys.get(index).cloned()
            );
        }
        assert!(row.get::<bool, _>("include_commits"));
    }
    let after = server
        .publish(vec![inline_welcome_envelope([55; 32])])
        .await?[0]
        .cursor
        .as_ref()
        .unwrap()
        .sequence_id as i64;
    sqlx::query("UPDATE allocation_boundary SET closed_sequence_id = $1")
        .bind(after)
        .execute(&server.backend.store.primary)
        .await?;
    let mut replacement = subscription(2, 3);
    replacement.hmac_epoch_base = 20;
    replacement.include_commits = false;
    let result = client
        .update_subscriptions(update(
            vec![replacement, subscription(4, 1)],
            vec![adds[0].topic.clone(), subscription(99, 0).topic],
        ))
        .await?
        .into_inner();
    assert_eq!(result.topic_count, 3);
    assert_count(&server, 3).await;
    let positions: Vec<(Vec<u8>, i64)> =
        sqlx::query_as("SELECT topic, since_sequence_id FROM push_subscription ORDER BY topic")
            .fetch_all(&server.backend.store.primary)
            .await?;
    assert_eq!(
        positions.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        [before, before, after]
    );
    let row = sqlx::query(
        "SELECT hmac_epoch_base, include_commits FROM push_subscription WHERE topic = $1",
    )
    .bind(subscription(2, 0).topic)
    .fetch_one(&server.backend.store.primary)
    .await?;
    assert_eq!(row.get::<i64, _>(0), 20);
    assert!(!row.get::<bool, _>(1));
    client
        .update_subscriptions(update(
            vec![subscription(2, 0), subscription(3, 1), subscription(4, 3)],
            vec![],
        ))
        .await?;
    assert_count(&server, 3).await;
    server.stop().await?;
}

// verifies: PUSH-217
#[xmtp_common::test(unwrap_try = true)]
async fn topic_limit_rolls_back_removes_and_adds_and_serializes_concurrent_updates() {
    let server = TestServer::new(|config| {
        configured(config);
        config.limits.max_push_topics = 2;
    })
    .await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    client
        .update_subscriptions(update(vec![subscription(1, 1), subscription(2, 1)], vec![]))
        .await?;
    let error = client
        .update_subscriptions(update(
            vec![subscription(3, 1), subscription(4, 1)],
            vec![subscription(1, 0).topic],
        ))
        .await
        .unwrap_err();
    status(
        error,
        Code::ResourceExhausted,
        "recipient topic limit reached",
    );
    assert_count(&server, 2).await;
    let topics: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT topic FROM push_subscription ORDER BY topic")
            .fetch_all(&server.backend.store.primary)
            .await?;
    assert_eq!(
        topics,
        vec![subscription(1, 0).topic, subscription(2, 0).topic]
    );
    client.update_subscriptions(update(vec![], topics)).await?;
    let mut other = server.notifications();
    let (left, right) = tokio::join!(
        client.update_subscriptions(update(vec![subscription(1, 1), subscription(2, 1)], vec![])),
        other.update_subscriptions(update(vec![subscription(3, 1), subscription(4, 1)], vec![]))
    );
    assert_ne!(left.is_ok(), right.is_ok());
    status(
        left.err().or(right.err()).unwrap(),
        Code::ResourceExhausted,
        "recipient topic limit reached",
    );
    assert_count(&server, 2).await;
    server.stop().await?;
}

// verifies: PUSH-207
#[xmtp_common::test(unwrap_try = true)]
async fn unregister_cascades_even_after_the_provider_is_removed() {
    let server = TestServer::new(configured).await?;
    let mut client = server.notifications();
    client.register(registration()).await?;
    client
        .update_subscriptions(update(vec![subscription(1, 1)], vec![]))
        .await?;
    let mut config = (*server.backend.config).clone();
    config.push.fcm = None;
    let mut second = RunningServer::new(config).await?;
    second.notifications().unregister(unregister()).await?;
    assert!(
        server
            .backend
            .store
            .load_recipient(&registration().recipient_id)
            .await?
            .is_none()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_subscription")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 0);
    status(
        client.unregister(unregister()).await.unwrap_err(),
        Code::NotFound,
        "recipient is not registered",
    );
    second.stop().await?;
    server.stop().await?;
}
