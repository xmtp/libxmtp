use super::*;
use crate::test_support::metrics::{isolated, value};
use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;

#[xmtp_common::test(unwrap_try = true)]
async fn dispatcher_records_bounded_delivery_outcomes_and_holder_lifecycle() {
    let Some(handle) = isolated(
        "push::tests::telemetry::dispatcher_records_bounded_delivery_outcomes_and_holder_lifecycle",
    ) else {
        return;
    };
    let mut fixture = Fixture::new().await?;
    fixture.config.push.max_attempts = 1;
    for id in 1..=6 {
        fixture
            .recipient(id, PushChannel::Http, &[id as u8], false, 0)
            .await?;
        fixture.seed(1, &[id as u8], false).await?;
    }
    let data = b"suppressed payload".to_vec();
    let payload = crate::api::ClientEnvelope {
        payload: Some(crate::api::client_envelope::Payload::GroupMessage(
            crate::api::GroupMessage {
                data: data.clone(),
                ..Default::default()
            },
        )),
    }
    .encode_to_vec();
    let key = vec![42u8; 42];
    let mut mac = Hmac::<Sha256>::new_from_slice(&key)?;
    mac.update(&data);
    sqlx::query("UPDATE envelopes SET payload = $1, sender_hmac = $2 WHERE sequence_id = 6")
        .bind(payload)
        .bind(mac.finalize().into_bytes().to_vec())
        .execute(&fixture.store.primary)
        .await?;
    sqlx::query(
        "UPDATE push_subscription SET hmac_epoch_base = 0, hmac_key_0 = $1 WHERE topic = $2",
    )
    .bind(&key)
    .bind(&[6u8][..])
    .execute(&fixture.store.primary)
    .await?;
    fixture.boundary(6).await?;
    let sender = Arc::new(FakeSender {
        outcomes: Mutex::new(VecDeque::from([
            Outcome::Delivered,
            Outcome::Rejected,
            Outcome::Mismatch,
            Outcome::Terminal,
            Outcome::Transient { retry_after: None },
        ])),
        ..Default::default()
    });
    let hub = fixture.hub(sender);
    xmtp_common::wait_for_eq(|| fixture.cursor(), 6).await?;
    assert_eq!(value(&handle, "xmtp_push_dispatcher", &[]), 1.0);
    for outcome in [
        "delivered",
        "rejected",
        "mismatch",
        "dead",
        "failed",
        "suppressed",
    ] {
        assert_eq!(
            value(
                &handle,
                "xmtp_push_deliveries_total",
                &[("channel", "http"), ("outcome", outcome)]
            ),
            1.0,
            "{outcome}"
        );
    }
    assert_eq!(
        value(&handle, "xmtp_push_recipients_total", &[("action", "dead")]),
        1.0
    );
    stop(&hub).await;
    assert_eq!(value(&handle, "xmtp_push_dispatcher", &[]), 0.0);
    let output = handle.render();
    for forbidden in [
        "push.invalid",
        "recipient_id=",
        "topic=",
        "metadata=",
        "signing_key=",
    ] {
        assert!(!output.contains(forbidden), "metric contains private field");
    }
}
