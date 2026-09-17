//! Sending, receiving, and commit basics.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;
    let messages = alix
        .context
        .api()
        .query_group_messages(group.group_id)
        .await?;

    group.sync().await?;
    let decrypted_messages = group.find_messages(&MsgQueryArgs::default())?;

    tracing::info!("The messages: {decrypted_messages:?}");

    // The AppDataUpdate proposal, the key-update commit, and the application message.
    assert_eq!(messages.len(), 3);
    let stored = decrypted_messages.last().unwrap();
    let envelope = messages.last().unwrap();
    assert!(envelope.envelope_hash.is_some());
    assert!(envelope.expiry_ns.is_some());
    assert_eq!(stored.envelope_hash, envelope.envelope_hash);
    assert_eq!(
        stored.expiry_ns,
        envelope.expiry_ns.map(|expiry| expiry as i64)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_self_message() {
    tester!(alix);
    let group = alix.create_group(None, None).expect("create group");
    let msg = b"hello";

    group
        .send_message(msg, SendMessageOpts::default())
        .await
        .expect("send message");

    group.receive().await?;
    // Check for messages
    let messages = group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_message_from_other() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None).expect("create group");
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let alix_message = b"hello from alix";
    alix_group
        .send_message(alix_message, SendMessageOpts::default())
        .await
        .expect("send message");

    let bo_group = receive_group_invite(&bo).await;
    let message = get_latest_message(&bo_group).await;
    assert_eq!(message.decrypted_message_bytes, alix_message);

    let bo_message = b"hello from bo";
    bo_group
        .send_message(bo_message, SendMessageOpts::default())
        .await
        .expect("send message");

    let message = get_latest_message(&alix_group).await;
    assert_eq!(message.decrypted_message_bytes, bo_message);
}

#[xmtp_common::test]
async fn test_key_update() {
    tester!(client);
    tester!(bola_client);

    let group = client.create_group(None, None).expect("create group");
    group.add_members(&[bola_client.inbox_id()]).await.unwrap();

    group.key_update().await.unwrap();

    let messages = client
        .context
        .api()
        .query_group_messages(group.group_id)
        .await
        .unwrap();
    // Adding Bola emits an Add proposal, a membership AppDataUpdate proposal, and a commit.
    // The explicit key update emits the fourth envelope.
    assert_eq!(messages.len(), 4);

    let pending_commit_is_none = group
        .load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
            Ok(mls_group.pending_commit().is_none())
        })
        .unwrap();

    assert!(pending_commit_is_none);

    group
        .send_message(b"hello", SendMessageOpts::default())
        .await
        .expect("send message");

    bola_client.sync_welcomes().await.unwrap();
    let bola_groups = bola_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bola_messages.len(), 2);
}

#[xmtp_common::test]
async fn test_post_commit() {
    tester!(client);
    tester!(client_2);
    let group = client.create_group(None, None).expect("create group");

    group.add_members(&[client_2.inbox_id()]).await.unwrap();

    // Check if the welcome was actually sent
    let welcome_messages = client
        .context
        .api()
        .query_welcome_messages(client_2.installation_public_key())
        .await
        .unwrap();

    assert_eq!(welcome_messages.len(), 1);
}

#[xmtp_common::test]
async fn test_staged_welcome() {
    // Create Clients
    tester!(amal);
    tester!(bola);

    // Amal creates a group
    let amal_group = amal.create_group(None, None).unwrap();

    // Amal adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_groups = bola.sync_welcomes().await.unwrap();

    // Bola gets the group id. This will be needed to fetch the group from
    // the database.
    let bola_group = bola_groups.first().unwrap();
    let bola_group_id = bola_group.group_id;

    // Bola fetches group from the database
    let bola_fetched_group = bola.group(&bola_group_id).unwrap();

    // Check Bola's group for the added_by_inbox_id of the inviter
    let added_by_inbox = bola_fetched_group.added_by_inbox_id().unwrap();

    // Verify the welcome host_credential is equal to Amal's
    assert_eq!(
        amal.inbox_id(),
        added_by_inbox,
        "The Inviter and added_by_address do not match!"
    );
}

#[xmtp_common::test]
async fn test_can_read_group_creator_inbox_id() {
    tester!(amal);
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(mutable_metadata.super_admin_list.len(), 1);
    assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

    let protected_metadata: GroupMetadata = amal_group.metadata().await.unwrap();
    assert_eq!(
        protected_metadata.conversation_type,
        ConversationType::Group
    );

    assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
}

#[xmtp_common::test]
async fn test_optimistic_send() {
    tester!(amal);
    tester!(bola);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.sync().await.unwrap();
    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();
    let bola_group = receive_group_invite(&bola).await;

    let ids = vec![
        amal_group
            .send_message_optimistic(b"test one", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test two", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test three", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test four", SendMessageOpts::default())
            .unwrap(),
    ];

    let messages = amal_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .collect::<Vec<StoredGroupMessage>>();

    let text = messages
        .iter()
        .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).to_string())
        .collect::<Vec<String>>();
    assert_eq!(
        ids,
        messages
            .iter()
            .cloned()
            .map(|m| m.id)
            .collect::<Vec<Vec<u8>>>()
    );
    assert_eq!(
        text,
        vec![
            "test one".to_string(),
            "test two".to_string(),
            "test three".to_string(),
            "test four".to_string(),
        ]
    );

    let delivery = messages
        .iter()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
        ]
    );

    amal_group.publish_messages().await.unwrap();
    bola_group.sync().await.unwrap();

    let messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let delivery = messages
        .iter()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
        ]
    );
}
