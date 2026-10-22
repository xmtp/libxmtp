use super::*;
use crate::{api, config::push::FcmConfig, test_support::TestServer};
use xmtp_mls_validation::test_utils::{GroupMessageKind, group_keys, keyed_group_message_envelope};
use xmtp_proto::types::TopicKind;

/// A keyed group's message topic is 33 bytes, as long as a welcome topic. A
/// subscription to it is accepted, and a signed message published to it
/// reaches its subscriber and no subscriber of the welcome topic that carries
/// the same 32 bytes, because the kind byte tells the two apart.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-215
async fn a_signed_message_on_a_keyed_group_topic_reaches_its_subscriber() {
    let server = TestServer::new(|config| {
        config.push.fcm = Some(FcmConfig {
            service_account: Some(channel::fcm::tests::service_account()),
        });
    })
    .await?;
    // Replace the server's dispatcher with one whose sender records deliveries.
    stop(server.backend.push.as_ref()?).await;
    let sender = Arc::new(FakeSender::default());
    let hub = PushHub::with_senders(
        (*server.backend.store).clone(),
        &server.backend.config,
        Arc::new(Notify::new()),
        Senders([None, Some(sender.clone()), None]),
    );
    let keys = group_keys(5);
    let group_topic = TopicKind::GroupMessagesV1.create(keys.group_id()).to_vec();
    let welcome_topic = TopicKind::WelcomeMessagesV1
        .create(keys.group_id())
        .to_vec();
    assert_eq!(group_topic.len(), 33);
    let mut client = server.notifications();
    for (recipient, topic) in [(1u8, group_topic.clone()), (2, welcome_topic)] {
        client
            .register(api::RegisterRequest {
                recipient_id: vec![recipient; 32],
                recipient_secret: vec![22; 32],
                delivery: Some(api::register_request::Delivery::Fcm(api::FcmDelivery {
                    token: "device-token".into(),
                })),
            })
            .await?;
        let added = client
            .update_subscriptions(api::UpdateSubscriptionsRequest {
                recipient_id: vec![recipient; 32],
                recipient_secret: vec![22; 32],
                adds: vec![api::Subscription {
                    topic,
                    hmac_epoch_base: 0,
                    hmac_keys: vec![],
                    include_commits: false,
                }],
                removes: vec![],
            })
            .await?
            .into_inner();
        assert_eq!(added.topic_count, 1);
    }
    let meta = server
        .publish(vec![keyed_group_message_envelope(
            &keys,
            GroupMessageKind::Application,
            [],
        )])
        .await?
        .remove(0);
    assert_eq!(meta.topic.as_ref()?.topic, group_topic);
    let sequence = meta.cursor.as_ref()?.sequence_id as i64;
    sqlx::query("UPDATE allocation_boundary SET closed_sequence_id = $1 WHERE singleton")
        .bind(sequence)
        .execute(&server.backend.store.primary)
        .await?;
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar::<_, i64>("SELECT sequence_id FROM push_cursor WHERE singleton")
                .fetch_one(&server.backend.store.primary)
                .await
                .unwrap()
        },
        sequence,
    )
    .await?;
    stop(&hub).await;
    assert_eq!(*sender.calls.lock(), [(sequence as u64, PushChannel::Fcm)]);
    server.stop().await?;
}
