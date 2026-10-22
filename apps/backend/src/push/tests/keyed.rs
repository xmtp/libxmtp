use super::*;
use crate::{api, config::push::FcmConfig, test_support::TestServer};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use xmtp_mls_common::group_keys::sign_group_message;
use xmtp_mls_validation::test_utils::{GroupMessageKind, group_keys, keyed_group_message_envelope};
use xmtp_proto::types::TopicKind;

/// A keyed group's message topic is 33 bytes, as long as a welcome topic. A
/// subscription to it is accepted, and a signed message published to it
/// reaches its subscriber and no subscriber of the welcome topic that carries
/// the same 32 bytes, because the kind byte tells the two apart. A second
/// subscriber of the group topic holds the sender's HMAC key, so the message
/// is its own and is suppressed for it.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PUSH-215, PUSH-230
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
    // The sender's key for the current HMAC epoch. The window holds it at
    // every index, so the message matches even across an epoch boundary.
    let hmac_key = vec![42u8; 42];
    let epoch = xmtp_common::time::now_ns()
        / (xmtp_push_types::HMAC_EPOCH_SECONDS * xmtp_common::NS_IN_SEC);
    let mut client = server.notifications();
    for (recipient, topic, hmac_keys) in [
        (1u8, group_topic.clone(), vec![]),
        (2, welcome_topic, vec![]),
        (3, group_topic.clone(), vec![hmac_key.clone(); 3]),
    ] {
        let hmac_epoch_base = if hmac_keys.is_empty() { 0 } else { epoch - 1 };
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
                    hmac_epoch_base,
                    hmac_keys,
                    include_commits: false,
                }],
                removes: vec![],
            })
            .await?
            .into_inner();
        assert_eq!(added.topic_count, 1);
    }
    let mut envelope = keyed_group_message_envelope(&keys, GroupMessageKind::Application, []);
    let Some(api::client_envelope::Payload::GroupMessage(message)) = envelope.payload.as_mut()
    else {
        unreachable!("the fixture is a group message")
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(&hmac_key)?;
    mac.update(&message.data);
    message.sender_hmac = mac.finalize().into_bytes().to_vec();
    sign_group_message(&keys, message)?;
    let meta = server.publish(vec![envelope]).await?.remove(0);
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
    assert_eq!(
        *sender.recipients.lock(),
        [vec![1; 32]],
        "only the group topic's subscriber without the sender's key"
    );
    server.stop().await?;
}
