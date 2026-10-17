use super::*;
use crate::{
    push::channel::{apns, fcm},
    test_support::{
        metrics::{isolated, value},
        push_provider::{Protocol, Provider, Reply},
    },
};
use serde_json::json;

async fn dispatch_one(
    fixture: &Fixture,
    id: u8,
    channel: PushChannel,
    token: &str,
    dead: bool,
) -> TestResult {
    let mut topic = vec![0; 17];
    topic[1] = id;
    let recipient = fixture
        .recipient(id.into(), channel, &topic, false, 0)
        .await?;
    sqlx::query("UPDATE push_recipient SET delivery = $1 WHERE recipient_id = $2")
        .bind(token)
        .bind(&recipient.recipient_id)
        .execute(&fixture.store.primary)
        .await?;
    let sequence = fixture.seed(1, &topic, false).await?[0];
    fixture.boundary(sequence).await?;
    xmtp_common::wait_for_eq(|| fixture.cursor(), sequence).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM push_recipient WHERE recipient_id = $1)")
            .bind(&recipient.recipient_id)
            .fetch_one(&fixture.store.primary)
            .await?;
    assert_eq!(exists, !dead, "recipient {id}");
    let subscription_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM push_subscription WHERE recipient_id = $1)",
    )
    .bind(&recipient.recipient_id)
    .fetch_one(&fixture.store.primary)
    .await?;
    assert_eq!(subscription_exists, !dead, "subscription {id}");
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn apns_terminal_replies_delete_but_sender_mismatches_keep_subscriptions_and_count() {
    let Some(metrics) = isolated(
        "push::tests::providers::apns_terminal_replies_delete_but_sender_mismatches_keep_subscriptions_and_count",
    ) else {
        return;
    };
    let mut fixture = Fixture::new().await?;
    fixture.config.push.max_attempts = 1;
    let provider = Provider::start(Protocol::Http2Tls, vec![Reply::json(200, json!({}))]).await?;
    let sender = Arc::new(apns::tests::sender(&provider)?);
    let hub = PushHub::with_senders(
        fixture.store.clone(),
        &fixture.config,
        Arc::new(Notify::new()),
        Senders([Some(sender), None, None]),
    );
    for (index, (status, reason, dead)) in [
        (410, "Unregistered", true),
        (410, "ExpiredToken", true),
        (400, "BadDeviceToken", false),
        (400, "DeviceTokenNotForTopic", false),
        (400, "Unregistered", false),
        (410, "UnknownReason", false),
        (403, "ExpiredProviderToken", false),
    ]
    .into_iter()
    .enumerate()
    {
        provider.set_replies(vec![Reply::json(status, json!({"reason": reason}))]);
        dispatch_one(
            &fixture,
            index as u8 + 1,
            PushChannel::Apns,
            &apns::tests::delivery().config.delivery,
            dead,
        )
        .await?;
    }
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "apns"), ("outcome", "mismatch")]
        ),
        2.0
    );
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "apns"), ("outcome", "dead")]
        ),
        2.0
    );
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_recipients_total",
            &[("action", "dead")]
        ),
        2.0
    );
    // A later successful answer reaches a retained subscription without Register.
    provider.set_replies(vec![Reply::json(200, json!({}))]);
    let mut topic = vec![0; 17];
    topic[1] = 3;
    let sequence = fixture.seed(1, &topic, false).await?[0];
    fixture.boundary(sequence).await?;
    xmtp_common::wait_for_eq(|| fixture.cursor(), sequence).await?;
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "apns"), ("outcome", "delivered")]
        ),
        1.0
    );
    stop(&hub).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn fcm_terminal_detail_deletes_but_mismatch_and_bare_statuses_keep_subscriptions_and_count() {
    let Some(metrics) = isolated(
        "push::tests::providers::fcm_terminal_detail_deletes_but_mismatch_and_bare_statuses_keep_subscriptions_and_count",
    ) else {
        return;
    };
    let mut fixture = Fixture::new().await?;
    fixture.config.push.max_attempts = 1;
    let token = Provider::start(Protocol::Http1, vec![fcm::tests::token_reply()]).await?;
    let provider = Provider::start(Protocol::Http1, vec![Reply::json(200, json!({}))]).await?;
    let sender = Arc::new(fcm::tests::sender(&token, &provider)?);
    let hub = PushHub::with_senders(
        fixture.store.clone(),
        &fixture.config,
        Arc::new(Notify::new()),
        Senders([None, Some(sender), None]),
    );
    for (index, (status, body, dead)) in [
        (404, fcm::tests::failure("UNREGISTERED"), true),
        (403, fcm::tests::failure("SENDER_ID_MISMATCH"), false),
        (404, json!({"error": {"status": "UNREGISTERED"}}), false),
        (
            403,
            json!({"error": {"status": "SENDER_ID_MISMATCH"}}),
            false,
        ),
        (400, fcm::tests::failure("INVALID_ARGUMENT"), false),
    ]
    .into_iter()
    .enumerate()
    {
        provider.set_replies(vec![Reply::json(status, body)]);
        dispatch_one(
            &fixture,
            index as u8 + 1,
            PushChannel::Fcm,
            &fcm::tests::delivery().config.delivery,
            dead,
        )
        .await?;
    }
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "fcm"), ("outcome", "mismatch")]
        ),
        1.0
    );
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "fcm"), ("outcome", "dead")]
        ),
        1.0
    );
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_recipients_total",
            &[("action", "dead")]
        ),
        1.0
    );
    provider.set_replies(vec![Reply::json(200, json!({}))]);
    let mut topic = vec![0; 17];
    topic[1] = 2;
    let sequence = fixture.seed(1, &topic, false).await?[0];
    fixture.boundary(sequence).await?;
    xmtp_common::wait_for_eq(|| fixture.cursor(), sequence).await?;
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "fcm"), ("outcome", "delivered")]
        ),
        1.0
    );
    stop(&hub).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn removed_provider_counts_failed_and_keeps_the_recipient() {
    let Some(metrics) =
        isolated("push::tests::providers::removed_provider_counts_failed_and_keeps_the_recipient")
    else {
        return;
    };
    let mut fixture = Fixture::new().await?;
    fixture.config.push.max_attempts = 10;
    let hub = PushHub::with_senders(
        fixture.store.clone(),
        &fixture.config,
        Arc::new(Notify::new()),
        Senders::default(),
    );
    dispatch_one(&fixture, 1, PushChannel::Fcm, "private-stored-token", false).await?;
    assert_eq!(
        value(
            &metrics,
            "xmtp_push_deliveries_total",
            &[("channel", "fcm"), ("outcome", "failed")]
        ),
        1.0
    );
    assert!(!metrics.render().contains("private-stored-token"));
    stop(&hub).await;
}
