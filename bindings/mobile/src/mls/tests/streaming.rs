//! Tests for message and conversation streaming

use super::*;

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_stream_group_messages_for_updates() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    // Stream all group messages
    let message_callbacks = Arc::new(RustStreamCallback::default());
    let stream_messages = bo
        .conversations()
        .stream_all_messages(message_callbacks.clone(), None)
        .await;
    stream_messages.wait_for_ready().await;

    // Create group and send first message
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    alix_group
        .update_group_name("Old Name".to_string())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    let bo_groups = bo
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    let bo_group = &bo_groups[0];
    bo_group.conversation.sync().await.unwrap();

    // alix published + processed group creation and name update
    assert_eq!(alix.client.inner_client.context.db().intents_published(), 2);
    assert_eq!(alix.client.inner_client.context.db().intents_processed(), 2);

    bo_group
        .conversation
        .update_group_name("Old Name2".to_string())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    assert_eq!(bo.client.inner_client.context.db().intents_published(), 1);

    alix_group
        .send(b"Hello there".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    assert_eq!(alix.client.inner_client.context.db().intents_published(), 3);

    let dm = bo
        .conversations()
        .find_or_create_dm_by_identity(
            alix.account_identifier.clone(),
            FfiCreateDMOptions::default(),
        )
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    dm.send(b"Hello again".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    assert_eq!(bo.client.inner_client.context.db().intents_published(), 3);
    message_callbacks.wait_for_delivery(None).await.unwrap();

    // Uncomment the following lines to add more group name updates
    bo_group
        .conversation
        .update_group_name("Old Name3".to_string())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    assert_eq!(bo.client.inner_client.context.db().intents_published(), 4);

    wait_for_eq(|| async { message_callbacks.message_count() }, 6)
        .await
        .unwrap();

    stream_messages.end_and_wait().await.unwrap();
    assert!(stream_messages.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_conversation_streaming() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    let stream_callback = Arc::new(RustStreamCallback::default());

    let stream = bola.conversations().stream(stream_callback.clone()).await;

    amal.conversations()
        .create_group_by_identity(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 1);
    // Create another group and add bola
    amal.conversations()
        .create_group_by_identity(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 2);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_all_messages() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;
    let caro = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![caro.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let stream_callback = Arc::new(RustStreamCallback::default());

    let stream = caro
        .conversations()
        .stream_all_messages(stream_callback.clone(), None)
        .await;
    stream.wait_for_ready().await;

    alix_group
        .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![caro.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    let _ = caro.inner_client.sync_welcomes().await.unwrap();

    bo_group
        .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    alix_group
        .send("third".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    bo_group
        .send("fourth".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 4);
    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_message_streaming() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    let amal_group: Arc<FfiConversation> = amal
        .conversations()
        .create_group_by_identity(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    bola.inner_client.sync_welcomes().await.unwrap();
    let bola_group = bola.conversation(amal_group.id()).unwrap();

    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream_closer = bola_group.stream(stream_callback.clone()).await;

    stream_closer.wait_for_ready().await;

    amal_group
        .send("hello".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    amal_group
        .send("goodbye".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 2);
    stream_closer.end_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_message_streaming_when_removed_then_added() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;
    log::info!(
        "Created Inbox IDs {} and {}",
        amal.inbox_id(),
        bola.inbox_id()
    );

    let amal_group = amal
        .conversations()
        .create_group_by_identity(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream_closer = bola
        .conversations()
        .stream_all_messages(stream_callback.clone(), None)
        .await;
    stream_closer.wait_for_ready().await;

    amal_group
        .send(b"hello1".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    amal_group
        .send(b"hello2".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 2);
    assert!(!stream_closer.is_closed());

    amal_group
        .remove_members(vec![bola.inbox_id().clone()])
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 3); // Member removal transcript message
    //
    amal_group
        .send(b"hello3".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    //TODO: could verify with a log message
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(stream_callback.message_count(), 3); // Don't receive messages while removed
    assert!(!stream_closer.is_closed());

    amal_group
        .add_members_by_identity(vec![bola.account_identifier.clone()])
        .await
        .unwrap();

    // TODO: could check for LOG message with a Eviction error on receive
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert_eq!(stream_callback.message_count(), 3); // Don't receive transcript messages while removed

    amal_group
        .send("hello4".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 4); // Receiving messages again
    assert!(!stream_closer.is_closed());

    stream_closer.end_and_wait().await.unwrap();
    assert!(stream_closer.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_groups_gets_callback_when_streaming_messages() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Stream all group messages
    let message_callback = Arc::new(RustStreamCallback::default());
    let group_callback = Arc::new(RustStreamCallback::default());
    let stream_groups = bo.conversations().stream(group_callback.clone()).await;

    let stream_messages = bo
        .conversations()
        .stream_all_messages(message_callback.clone(), None)
        .await;
    stream_messages.wait_for_ready().await;

    // Create group and send first message
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    group_callback.wait_for_delivery(None).await.unwrap();

    alix_group
        .send("hello1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(group_callback.message_count(), 1);
    assert_eq!(message_callback.message_count(), 1);

    stream_messages.end_and_wait().await.unwrap();
    assert!(stream_messages.is_closed());

    stream_groups.end_and_wait().await.unwrap();
    assert!(stream_groups.is_closed());
}

#[cfg_attr(feature = "d14n", ignore)]
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_consent() {
    let alix_a = Tester::builder().sync_worker().sync_server().build().await;

    let alix_b = alix_a.builder.build().await;

    let bo = Tester::new().await;

    // check that they have the same sync group
    alix_a
        .inner_client
        .test_has_same_sync_group_as(&alix_b.inner_client)
        .await
        .unwrap();

    alix_a
        .worker()
        .register_interest(SyncMetric::PayloadTaskScheduled, 1)
        .wait()
        .await
        .unwrap();

    alix_a
        .worker()
        .register_interest(SyncMetric::PayloadSent, 1)
        .wait()
        .await
        .unwrap();
    alix_a
        .worker()
        .register_interest(SyncMetric::HmacSent, 1)
        .wait()
        .await
        .unwrap();

    alix_b.sync_all_device_sync_groups().await.unwrap();
    alix_b
        .worker()
        .register_interest(SyncMetric::PayloadProcessed, 1)
        .wait()
        .await
        .unwrap();
    alix_a
        .inner_client
        .test_has_same_sync_group_as(&alix_b.inner_client)
        .await
        .unwrap();
    alix_b
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1)
        .wait()
        .await
        .unwrap();

    // create a stream from both installations
    let stream_a_callback = Arc::new(RustStreamCallback::default());
    let stream_b_callback = Arc::new(RustStreamCallback::default());
    let a_stream = alix_a
        .conversations()
        .stream_consent(stream_a_callback.clone())
        .await;
    let b_stream = alix_b
        .conversations()
        .stream_consent(stream_b_callback.clone())
        .await;
    a_stream.wait_for_ready().await;
    b_stream.wait_for_ready().await;
    alix_b.sync_all_device_sync_groups().await.unwrap();

    // consent with bo
    alix_a
        .set_consent_states(vec![FfiConsent {
            entity: bo.inbox_id(),
            entity_type: FfiConsentEntityType::InboxId,
            state: FfiConsentState::Denied,
        }])
        .await
        .unwrap();

    // Wait for alix_a to send the consent sync out
    alix_a
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await
        .unwrap();

    // Have alix_b sync the sync group and wait for the new consent to be processed
    alix_b.sync_all_device_sync_groups().await.unwrap();
    alix_b
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await
        .unwrap();

    stream_a_callback.wait_for_delivery(Some(3)).await.unwrap();
    wait_for_ok(|| async {
        alix_b.sync_all_device_sync_groups().await.unwrap();
        stream_b_callback.wait_for_delivery(Some(1)).await
    })
    .await
    .unwrap();

    wait_for_eq(|| async { stream_a_callback.consent_updates_count() }, 1)
        .await
        .unwrap();
    wait_for_eq(|| async { stream_a_callback.consent_updates_count() }, 1)
        .await
        .unwrap();

    // Consent should be the same
    let consent_a = alix_a
        .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
        .await
        .unwrap();
    let consent_b = alix_b
        .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
        .await
        .unwrap();
    assert_eq!(consent_a, consent_b);

    // Now we'll allow Bo
    alix_a
        .set_consent_states(vec![FfiConsent {
            entity: bo.inbox_id(),
            entity_type: FfiConsentEntityType::InboxId,
            state: FfiConsentState::Allowed,
        }])
        .await
        .unwrap();

    // Wait for alix_a to send out the consent on the sync group
    alix_a
        .worker()
        .register_interest(SyncMetric::ConsentSent, 3)
        .wait()
        .await
        .unwrap();
    // Have alix_b sync the sync group
    alix_b.sync_all_device_sync_groups().await.unwrap();
    // Wait for alix_b to process the new consent
    alix_b
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 2)
        .wait()
        .await
        .unwrap();

    // This consent should stream
    wait_for_ge(|| async { stream_a_callback.consent_updates_count() }, 2)
        .await
        .unwrap();

    // alix_b should now be ALLOWED with bo via device sync
    let consent_b = alix_b
        .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
        .await
        .unwrap();
    assert_eq!(consent_b, FfiConsentState::Allowed);

    a_stream.end_and_wait().await.unwrap();
    b_stream.end_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_preferences() {
    let alix_wallet = generate_local_wallet();
    let alix_a_span = info_span!("alix_a");
    let alix_a = Tester::builder()
        .owner(alix_wallet.clone())
        .sync_worker()
        .with_name("alix_a")
        .build()
        .instrument(alix_a_span)
        .await;
    let alix_b_span = info_span!("alix_b");
    let alix_b = Tester::builder()
        .owner(alix_wallet)
        .sync_worker()
        .with_name("alix_b")
        .build()
        .instrument(alix_b_span)
        .await;

    let hmac_sent = alix_a.worker().register_interest(SyncMetric::HmacSent, 1);
    let hmac_received = alix_b
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1);

    let cb = RustStreamCallback::default();
    let notify = cb.enable_notifications();
    tokio::pin!(notify);
    notify.as_mut().enable();

    let stream_b_callback = Arc::new(cb);
    let b_stream = alix_b
        .conversations()
        .stream_preferences(stream_b_callback.clone())
        .await;

    b_stream.wait_for_ready().await;

    alix_a
        .inner_client
        .test_has_same_sync_group_as(&alix_b.inner_client)
        .await
        .unwrap();

    hmac_sent.wait().await.unwrap();
    alix_b.sync_all_device_sync_groups().await.unwrap();
    hmac_received.wait().await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(10), notify).await;
    assert!(result.is_ok());

    {
        let updates = stream_b_callback.preference_updates.lock();
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, FfiPreferenceUpdate::HMAC { .. }))
        );
    }

    b_stream.end_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_overlapping_streams() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    let message_callbacks = Arc::new(RustStreamCallback::default());
    let conversation_callbacks = Arc::new(RustStreamCallback::default());
    // Stream all group messages
    let stream_messages = bo.conversations().stream(message_callbacks.clone()).await;
    // Stream all groups
    let stream_conversations = bo
        .conversations()
        .stream(conversation_callbacks.clone())
        .await;
    stream_messages.wait_for_ready().await;
    stream_conversations.wait_for_ready().await;

    // Create group and send first message
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    alix_group
        .send("hi".into(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // The group should be received in both streams without erroring
    message_callbacks.wait_for_delivery(None).await.unwrap();
    conversation_callbacks
        .wait_for_delivery(None)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_stream_and_update_name_without_forking_group() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Stream all group messages
    let message_callbacks = Arc::new(RustStreamCallback::default());
    let stream_messages = bo
        .conversations()
        .stream_all_messages(message_callbacks.clone(), None)
        .await;
    stream_messages.wait_for_ready().await;

    let first_msg_check = 2;
    let second_msg_check = 5;

    // Create group and send first message
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    if cfg!(feature = "d14n") {
        // give time for d14n to catch up
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    alix_group
        .update_group_name("hello".to_string())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    alix_group
        .send("hello1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    let bo_groups = bo
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(bo_groups.len(), 1);
    let bo_group = bo_groups[0].clone();
    bo_group.conversation.sync().await.unwrap();

    let bo_messages1 = bo_group
        .conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages1.len(), first_msg_check + 1);

    bo_group
        .conversation
        .send("hello2".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    bo_group
        .conversation
        .send("hello3".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    alix_group.sync().await.unwrap();

    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(alix_messages.len(), second_msg_check);

    alix_group
        .send("hello4".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();
    bo_group.conversation.sync().await.unwrap();

    let bo_messages2 = bo_group
        .conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages2.len(), second_msg_check + 1);
    assert_eq!(message_callbacks.message_count(), second_msg_check as u32);

    stream_messages.end_and_wait().await.unwrap();
    assert!(stream_messages.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_all_messages_with_optimistic_group_creation() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Start streaming FIRST (before any groups are created)
    let message_callbacks = Arc::new(RustStreamCallback::default());
    let stream_messages = bo
        .conversations()
        .stream_all_messages(message_callbacks.clone(), None)
        .await;
    stream_messages.wait_for_ready().await;

    // Create a group optimistically
    let alix_group = alix
        .conversations()
        .create_group_optimistic(FfiCreateGroupOptions::default())
        .unwrap();

    // add bo
    alix_group
        .add_members_by_identity(vec![bo.account_identifier.clone()])
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    alix_group
        .send(
            "first message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    // Create ANOTHER optimistic group (stress test for vector clock logic)
    let alix_group_2 = alix
        .conversations()
        .create_group_optimistic(FfiCreateGroupOptions::default())
        .unwrap();
    alix_group_2
        .add_members_by_identity(vec![bo.account_identifier.clone()])
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Send messages in the second group
    alix_group_2
        .send(
            "second group message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    alix_group
        .send(
            "third message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    message_callbacks.wait_for_delivery(None).await.unwrap();

    // Verify stream received all 3 application messages without "killing" itself
    // stream must continue to work after optimistic group creation
    assert_eq!(message_callbacks.message_count(), 3);

    stream_messages.end_and_wait().await.unwrap();
    assert!(stream_messages.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_message_deletions_with_full_message_details() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Create a group
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Send a properly encoded text message
    let message_id = alix_group
        .send(
            encode_text("Hello, world!".to_string()).unwrap(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Set up the deletion stream
    let deletion_callback = Arc::new(RustMessageDeletionCallback::default());
    let stream = alix
        .conversations()
        .stream_message_deletions(deletion_callback.clone())
        .await;
    stream.wait_for_ready().await;

    // Delete the message
    let deleted_count = alix.delete_message(message_id.clone()).unwrap();
    assert_eq!(deleted_count, 1);

    // Wait for stream to receive the deleted message
    deletion_callback.wait_for_delivery(Some(5)).await.unwrap();

    // Verify the stream received the deleted message with full details
    assert_eq!(deletion_callback.deleted_message_count(), 1);
    let deleted_messages = deletion_callback.deleted_messages();
    assert_eq!(deleted_messages[0].id(), message_id);
    assert_eq!(deleted_messages[0].sender_inbox_id(), alix.inbox_id());
    assert_eq!(deleted_messages[0].conversation_id(), alix_group.id());

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());
}
