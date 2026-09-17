//! Adding and removing members.

use super::*;

#[xmtp_common::test]
async fn test_members_func_from_non_creator() {
    tester!(amal);
    tester!(bola);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Get bola's version of the same group
    let bola_groups = bola.sync_welcomes().await.unwrap();
    let bola_group = bola_groups.first().unwrap();

    // Call sync for both
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify bola can see the group name
    let bola_group_name = bola_group.group_name().unwrap();
    assert_eq!(bola_group_name, "");

    // Check if both clients can see the members correctly
    let amal_members: Vec<GroupMember> = amal_group.members().await.unwrap();
    let bola_members: Vec<GroupMember> = bola_group.members().await.unwrap();

    assert_eq!(amal_members.len(), 2);
    assert_eq!(bola_members.len(), 2);

    for member in &amal_members {
        if member.inbox_id == amal.inbox_id() {
            assert_eq!(
                member.permission_level,
                PermissionLevel::SuperAdmin,
                "Amal should be a super admin"
            );
        } else if member.inbox_id == bola.inbox_id() {
            assert_eq!(
                member.permission_level,
                PermissionLevel::Member,
                "Bola should be a member"
            );
        }
    }
}

#[xmtp_common::test]
async fn test_add_member_conflict() {
    tester!(amal);
    tester!(bola);
    tester!(charlie);

    let amal_group = amal.create_group(None, None).unwrap();
    // Add bola
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Get bola's version of the same group
    let bola_groups = bola.sync_welcomes().await.unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    tracing::info!("Adding charlie from amal");
    // Have amal and bola both invite charlie.
    amal_group
        .add_members(&[charlie.inbox_id()])
        .await
        .expect("failed to add charlie");
    let added_authenticator = amal_group.epoch_authenticator().await.unwrap();
    tracing::info!("Adding charlie from bola");
    bola_group
        .add_members(&[charlie.inbox_id()])
        .await
        .expect("bola's add should succeed in a no-op");

    let summary = amal_group.receive().await.unwrap();
    assert!(!summary.is_errored());
    assert!(summary.errored.is_empty());
    assert_eq!(
        amal_group.epoch_authenticator().await.unwrap(),
        added_authenticator
    );
    assert_eq!(
        bola_group.epoch_authenticator().await.unwrap(),
        added_authenticator
    );

    // Check Amal's MLS group state.
    let amal_db = amal.context.db();
    let amal_members_len = amal_group
        .load_mls_group_with_lock(amal.context.mls_storage(), |mls_group| {
            Ok(mls_group.members().count())
        })
        .unwrap();

    assert_eq!(amal_members_len, 3);

    // Check Bola's MLS group state.
    let bola_db = bola.context.db();
    let bola_members_len = bola_group
        .load_mls_group_with_lock(bola.context.mls_storage(), |mls_group| {
            Ok(mls_group.members().count())
        })
        .unwrap();

    assert_eq!(bola_members_len, 3);

    let amal_uncommitted_intents = amal_db
        .find_group_intents(
            amal_group.group_id,
            Some(vec![
                IntentState::ToPublish,
                IntentState::Published,
                IntentState::Error,
            ]),
            None,
        )
        .unwrap();
    assert_eq!(amal_uncommitted_intents.len(), 0);

    let bola_failed_intents = bola_db
        .find_group_intents(bola_group.group_id, Some(vec![IntentState::Error]), None)
        .unwrap();
    // Bola's attempted add should be deleted, since it will have been a no-op on the second try
    assert_eq!(bola_failed_intents.len(), 0);

    // Make sure sending and receiving both worked
    amal_group
        .send_message("hello from amal".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    bola_group
        .send_message("hello from bola".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let matching_message = bola_messages
        .iter()
        .find(|m| m.decrypted_message_bytes == "hello from amal".as_bytes());
    tracing::info!("found message: {:?}", bola_messages);
    assert!(matching_message.is_some());
}

#[xmtp_common::test]
async fn test_add_inbox() {
    tester!(client);
    tester!(client_2);
    let group = client.create_group(None, None).expect("create group");

    group.add_members(&[client_2.inbox_id()]).await.unwrap();

    let group_id = group.group_id;

    let messages = client
        .context
        .api()
        .query_group_messages(group_id)
        .await
        .unwrap();
    // Adding a member emits an Add proposal, a membership AppDataUpdate proposal,
    // and the commit that consumes both proposals.
    assert_eq!(messages.len(), 3);
}

#[xmtp_common::test]
async fn test_add_invalid_member() {
    tester!(client);
    let group = client.create_group(None, None).expect("create group");

    let result = group.add_members(&["1234".to_string()]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_add_unregistered_member() {
    tester!(amal);
    let unconnected_ident = Identifier::rand_ethereum();
    let group = amal.create_group(None, None).unwrap();
    let result = group.add_members_by_identity(&[unconnected_ident]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_remove_inbox() {
    tester!(client_1);
    // Add another client onto the network
    tester!(client_2);

    let group = client_1.create_group(None, None).expect("create group");
    group
        .add_members(&[client_2.inbox_id()])
        .await
        .expect("group create failure");

    let messages_with_add = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_add.len(), 1);

    // Try and add another member without merging the pending commit
    group
        .remove_members(&[client_2.inbox_id()])
        .await
        .expect("group remove members failure");

    let messages_with_remove = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_remove.len(), 2);

    // Each membership update emits an MLS proposal, a membership AppDataUpdate proposal,
    // and a commit. Both the add and remove are published.
    let group_id = group.group_id;
    let messages = client_1
        .context
        .api()
        .query_group_messages(group_id)
        .await
        .expect("read topic");

    assert_eq!(messages.len(), 6);
}

#[xmtp_common::test]
async fn test_remove_by_account_address() {
    tester!(amal);
    tester!(bola);
    tester!(charlie);

    let group = amal.create_group(None, None).unwrap();
    group
        .add_members_by_identity(&[bola.identifier(), charlie.identifier()])
        .await
        .unwrap();
    tracing::info!("created the group with 2 additional members");
    assert_eq!(group.members().await.unwrap().len(), 3);
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(messages[0].decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 2);
    assert_eq!(group_update.removed_inboxes.len(), 0);
    assert_eq!(group_update.left_inboxes.len(), 0);

    group
        .remove_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();
    assert_eq!(group.members().await.unwrap().len(), 2);
    tracing::info!("removed bola");
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(messages[1].decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 0);
    assert_eq!(group_update.removed_inboxes.len(), 1);
    assert_eq!(group_update.left_inboxes.len(), 0);

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(!bola_group.is_active().unwrap())
}

#[xmtp_common::test]
async fn test_removed_members_cannot_send_message_to_others() {
    tester!(amal);
    tester!(bola);
    tester!(charlie);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members_by_identity(&[bola.identifier(), charlie.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 3);

    amal_group
        .remove_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    assert!(
        amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .all(|m| m.inbox_id != bola.inbox_id())
    );
    assert!(
        amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .any(|m| m.inbox_id == charlie.inbox_id())
    );

    amal_group.sync().await.expect("sync failed");

    let message_text = b"hello";

    let bola_group = TestMlsGroup::new(
        bola.context.clone(),
        amal_group.group_id,
        amal_group.dm_id.clone(),
        amal_group.conversation_type,
        amal_group.created_at_ns,
    );
    bola_group
        .send_message(message_text, SendMessageOpts::default())
        .await
        .expect_err("expected send_message to fail");

    amal_group.sync().await.expect("sync failed");
    amal_group.sync().await.expect("sync failed");

    let amal_messages = amal_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .collect::<Vec<StoredGroupMessage>>();

    assert!(amal_messages.is_empty());
}

#[xmtp_common::test]
async fn test_add_missing_installations() {
    // Setup for test
    tester!(amal);
    tester!(bola);

    let group = amal.create_group(None, None).unwrap();
    group.add_members(&[bola.inbox_id()]).await.unwrap();

    assert_eq!(group.members().await.unwrap().len(), 2);
    // Finished with setup

    // add a second installation for amal using the same wallet
    tester!(_amal_2nd, from: amal);

    // test if adding the new installation(s) worked
    let new_installations_were_added = group.add_missing_installations().await;
    assert!(new_installations_were_added.is_ok());

    group.sync().await.unwrap();
    let num_members = group
        .load_mls_group_with_lock(amal.context.mls_storage(), |mls_group| {
            Ok(mls_group.members().collect::<Vec<_>>().len())
        })
        .unwrap();

    assert_eq!(num_members, 3);
}

#[xmtp_common::test]
#[ignore]
async fn test_max_limit_add() {
    tester!(amal);
    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            None,
        )
        .unwrap();
    let mut clients = Vec::new();
    for _ in 0..249 {
        tester!(alix);
        clients.push(alix.identifier());
    }
    amal_group.add_members_by_identity(&clients).await.unwrap();
    tester!(bola);
    assert!(amal_group.add_members(&[bola.inbox_id()]).await.is_err(),);
}
