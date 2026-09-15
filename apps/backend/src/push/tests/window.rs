use super::*;
use crate::api;
use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;
use std::collections::HashMap;

#[xmtp_common::test(unwrap_try = true)]
async fn empty_fanout_preserves_bounds_and_never_reads_payloads() {
    let fixture = Fixture::new().await?;
    fixture.seed(4, &[1], false).await?;
    let mut cache = HashMap::new();
    let page = super::super::window::load(&fixture.store, 0, 4, &(0, vec![]), &mut cache).await?;
    assert_eq!((page.first, page.last), (Some(1), Some(4)));
    assert!(page.deliveries.is_empty());
    assert!(!page.full);
    assert!(cache.is_empty());
    fixture.boundary(4).await?;
    let sender = Arc::new(FakeSender::default());
    let hub = fixture.hub(sender.clone());
    xmtp_common::wait_for_eq(|| fixture.cursor(), 4).await?;
    assert_eq!(sender.count(), 0);
    stop(&hub).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn keyset_pages_keep_subscribers_and_start_commit_filters() {
    let fixture = Fixture::new().await?;
    fixture.seed(1, &[1], true).await?;
    fixture.seed(1, &[1], false).await?;
    // One thousand and one recipients exercise a partial second page.
    sqlx::query("INSERT INTO push_recipient (recipient_id, secret_hash, channel, delivery, signing_key, metadata, topic_count, renewed_ns) SELECT decode(lpad(to_hex(id), 64, '0'), 'hex'), decode(repeat('01', 32), 'hex'), 3, 'https://push.invalid', decode(repeat('07',32),'hex'), ''::bytea, 1, 1 FROM generate_series(1,1001) id")
        .execute(&fixture.store.primary).await?;
    sqlx::query("INSERT INTO push_subscription (recipient_id, topic, since_sequence_id, include_commits) SELECT recipient_id, $1, 0, false FROM push_recipient")
        .bind(&[1u8][..]).execute(&fixture.store.primary).await?;
    fixture
        .recipient(2000, PushChannel::Http, &[1], true, 0)
        .await?;
    fixture
        .recipient(2001, PushChannel::Http, &[1], true, 2)
        .await?;
    let mut cache = HashMap::new();
    let first = super::super::window::load(&fixture.store, 0, 2, &(0, vec![]), &mut cache).await?;
    assert_eq!(first.deliveries.len(), 1000);
    assert!(first.full);
    let second = super::super::window::load(&fixture.store, 0, 2, &first.next, &mut cache).await?;
    assert_eq!(second.deliveries.len(), 3);
    assert!(!second.full);
    let mut sequences: Vec<_> = first
        .deliveries
        .iter()
        .chain(&second.deliveries)
        .map(|delivery| delivery.payload.sequence_id.as_str())
        .collect();
    sequences.sort_unstable();
    assert_eq!(sequences.iter().filter(|&&id| id == "1").count(), 1);
    assert_eq!(sequences.iter().filter(|&&id| id == "2").count(), 1002);
    assert!(cache.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn hmac_uses_each_epoch_key_without_merging_other_messages() {
    let fixture = Fixture::new().await?;
    let config = fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    fixture
        .recipient(2, PushChannel::Http, &[1], false, 0)
        .await?;
    let keys = [vec![11u8; 42], vec![12u8; 42], vec![13u8; 42]];
    sqlx::query("UPDATE push_subscription SET hmac_epoch_base = 9, hmac_key_0 = $2, hmac_key_1 = $3, hmac_key_2 = $4 WHERE recipient_id = $1")
        .bind(&config.recipient_id).bind(&keys[0]).bind(&keys[1]).bind(&keys[2]).execute(&fixture.store.primary).await?;
    let ids = fixture.seed(5, &[1], false).await?;
    for (index, id) in ids.iter().enumerate() {
        let data = vec![index as u8; 8];
        let payload = api::ClientEnvelope {
            payload: Some(api::client_envelope::Payload::GroupMessage(
                api::GroupMessage {
                    data: data.clone(),
                    ..Default::default()
                },
            )),
        }
        .encode_to_vec();
        let mut mac = Hmac::<Sha256>::new_from_slice(&keys[index.min(2)])?;
        mac.update(&data);
        let hmac = if index == 4 {
            None
        } else {
            Some(mac.finalize().into_bytes().to_vec())
        };
        let server_ns =
            (9 + index as i64) * xmtp_push_types::HMAC_EPOCH_SECONDS * xmtp_common::NS_IN_SEC;
        sqlx::query("UPDATE envelopes SET payload = $2, sender_hmac = $3, server_ns = $4 WHERE sequence_id = $1")
            .bind(id).bind(payload).bind(hmac).bind(server_ns).execute(&fixture.store.primary).await?;
    }
    let mut cache = HashMap::new();
    let page = super::super::window::load(&fixture.store, 0, 5, &(0, vec![]), &mut cache).await?;
    assert_eq!(page.suppressed.len(), 3);
    assert_eq!(page.deliveries.len(), 7);
    assert_eq!(
        cache.len(),
        3,
        "missing keys and absent HMAC must not load payloads"
    );
    let own: Vec<_> = page
        .deliveries
        .iter()
        .filter(|delivery| delivery.config.recipient_id == config.recipient_id)
        .map(|delivery| delivery.payload.sequence_id.as_str())
        .collect();
    assert_eq!(own, ["4", "5"]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn terminal_response_compares_every_delivery_field() {
    let fixture = Fixture::new().await?;
    let original = fixture
        .recipient(1, PushChannel::Http, &[1], false, 0)
        .await?;
    for field in 0..4 {
        let mut attempted = original.clone();
        match field {
            0 => attempted.channel = PushChannel::Apns,
            1 => attempted.delivery.push_str("/old"),
            2 => attempted.signing_key = Some(vec![99; 32]),
            _ => attempted.metadata.push(99),
        }
        assert!(!dispatcher::delete_dead(&fixture.store, &attempted).await?);
    }
    assert!(dispatcher::delete_dead(&fixture.store, &original).await?);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_subscription")
        .fetch_one(&fixture.store.primary)
        .await?;
    assert_eq!(count, 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn stale_cursor_compare_and_set_cannot_advance() {
    let fixture = Fixture::new().await?;
    let mut connection = crate::db::dedicated_read(&fixture.store.primary, 1000).await?;
    let mut previous = 0;
    dispatcher::persist(&mut connection, &mut previous, 5).await?;
    assert_eq!(previous, 5);
    dispatcher::persist(&mut connection, &mut previous, 4).await?;
    assert_eq!(fixture.cursor().await, 5);
    let mut stale = 0;
    assert!(
        dispatcher::persist(&mut connection, &mut stale, 8)
            .await
            .is_err()
    );
    assert_eq!(fixture.cursor().await, 5);
}
