//! Sending, receiving, and commit basics.

use super::*;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};
use xmtp_events::{ClientEvent, EventFilter, EventKind, MessageStatus};

mod events;

#[derive(Clone, Copy, Debug)]
enum RefreshCase {
    SuperAdmin,
    Member,
    DmRevocation,
    DmAddition,
}

// Port of the 5adea8f7 evidence test, with both DM identity-change paths.
// verifies: GMOD-050, PERM-003
#[rstest::rstest]
#[case::super_admin_refreshes_own_revoked_installation(RefreshCase::SuperAdmin)]
#[case::member_refreshes_another_inbox_under_admin_only_policy(RefreshCase::Member)]
#[case::dm_member_refreshes_after_peer_revokes_device(RefreshCase::DmRevocation)]
#[case::dm_member_adds_own_new_installation(RefreshCase::DmAddition)]
#[xmtp_common::test(unwrap_try = true)]
async fn revoked_installation_refresh_reaches_every_member(
    #[case] case: RefreshCase,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let is_dm = matches!(case, RefreshCase::DmRevocation | RefreshCase::DmAddition);
    let addition = matches!(case, RefreshCase::DmAddition);
    let group = if is_dm {
        alix.find_or_create_dm(bo.inbox_id(), None).await?
    } else {
        let policies = matches!(case, RefreshCase::Member)
            .then(|| PreconfiguredPolicies::AdminsOnly.to_policy_set());
        alix.create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], policies, None)
            .await?
    };
    let bo_group = receive_group_invite(&bo).await;
    let caro_group = if is_dm {
        None
    } else {
        Some(receive_group_invite(&caro).await)
    };
    let owner = match case {
        RefreshCase::SuperAdmin | RefreshCase::DmAddition => &alix,
        RefreshCase::Member => &caro,
        RefreshCase::DmRevocation => &bo,
    };
    let device = owner.new_installation().await;
    let device_id = device.context.installation_id().to_vec();
    let revoked_group = if addition {
        None
    } else {
        group.add_missing_installations().await?;
        bo_group.sync().await?;
        if let Some(peer) = &caro_group {
            peer.sync().await?;
        }
        let device_group = receive_group_invite(&device).await;
        revoke(owner, &device_id).await;
        Some(device_group)
    };
    let refresher = if matches!(case, RefreshCase::Member) {
        &bo_group
    } else {
        &group
    };
    refresher.add_missing_installations().await?;
    group.sync().await?;
    bo_group.sync().await?;
    if let Some(peer) = &caro_group {
        peer.sync().await?;
    }
    let authenticator = group.epoch_authenticator().await?;
    for peer in [&group, &bo_group].into_iter().chain(caro_group.as_ref()) {
        assert_eq!(peer.epoch_authenticator().await?, authenticator);
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok::<_, GroupError>(
                mls.members()
                    .any(|member| member.signature_key == device_id)
            ))?,
            addition
        );
    }
    if let Some(revoked_group) = revoked_group {
        revoked_group.sync().await?;
        assert!(!revoked_group.is_active()?);
    } else {
        let added_group = receive_group_invite(&device).await;
        assert!(added_group.is_active()?);
        assert_eq!(added_group.epoch_authenticator().await?, authenticator);
    }
    group
        .send_message(b"after refresh", Default::default())
        .await?;
    bo_group.sync().await?;
    bo_group
        .send_message(b"peer after refresh", Default::default())
        .await?;
    group.sync().await?;
    assert_eq!(
        group.test_last_message_bytes().await?,
        Some(b"peer after refresh".to_vec())
    );
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message() {
    let alix = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let client = EngineTestPeer::new().await;
    let bola_client = EngineTestPeer::new().await;

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
    let client = EngineTestPeer::new().await;
    let client_2 = EngineTestPeer::new().await;
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
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

    // Amal creates a group
    let amal_group = amal.create_group(None, None).unwrap();

    // Amal adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_groups = bola.wait_for_welcomes().await.unwrap();

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
async fn test_optimistic_send() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

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

// A frozen send clock exercises the WASM path on native as well as WASM.
#[xmtp_common::test(unwrap_try = true)]
async fn same_millisecond_optimistic_sends_keep_recovery_order() {
    use crate::groups::message_id::calculate_message_id;
    use xmtp_db::delivery::QueryDelivery;
    use xmtp_db::group_message::{RecoveryQueryArgs, SortDirection};

    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    // Select keys whose IDs sort opposite to send order, so raw time fails reliably.
    let content = b"same millisecond";
    let mut keys = ["first", "second"];
    if calculate_message_id(group.group_id, content, keys[0])
        < calculate_message_id(group.group_id, content, keys[1])
    {
        keys.swap(0, 1);
    }
    let wall_ns = xmtp_common::time::now_ns() / 1_000_000 * 1_000_000;
    alix.context.freeze_message_clock(wall_ns);
    let send = |key: &str| {
        group.send_message_optimistic(
            content,
            SendMessageOpts {
                idempotency_key: Some(key.into()),
                ..Default::default()
            },
        )
    };
    let first = send(keys[0])?;
    let second = send(keys[1])?;
    assert!(first > second);
    let db = alix.context.db();
    let query = |direction, after| RecoveryQueryArgs {
        messages: MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            limit: Some(1),
            direction: Some(direction),
            ..Default::default()
        },
        after,
        ..Default::default()
    };
    let page = db.recovery_page_rows(&group.group_id, &query(SortDirection::Ascending, None))?;
    assert_eq!(page.rows[0].stored.id, first);
    assert_eq!(page.rows[0].stored.sent_at_ns, wall_ns);
    assert!(page.has_more);
    let next = db.recovery_page_rows(
        &group.group_id,
        &query(SortDirection::Ascending, page.last_position),
    )?;
    assert_eq!(next.rows[0].stored.id, second);
    assert_eq!(next.rows[0].stored.sent_at_ns, wall_ns + 1);
    assert!(!next.has_more);
    let descending =
        db.recovery_page_rows(&group.group_id, &query(SortDirection::Descending, None))?;
    assert_eq!(descending.rows[0].stored.id, second);
    // An idempotent retry retains the original message and timestamp.
    assert_eq!(send(keys[0])?, first);
    assert_eq!(db.get_group_message(&first)?.unwrap().sent_at_ns, wall_ns);
}

#[derive(Clone, Copy, Debug)]
enum PastEpochSender {
    Present,
    Removed,
    LeafReusedBySibling,
}

// verifies: PROC-059
#[rstest::rstest]
#[case::present_member_one_epoch_behind(PastEpochSender::Present)]
#[case::removed_member(PastEpochSender::Removed)]
#[case::revoked_installation_leaf_reused_by_sibling(PastEpochSender::LeafReusedBySibling)]
#[xmtp_common::test(unwrap_try = true)]
async fn retained_epoch_sender_must_still_hold_its_leaf(
    #[case] case: PastEpochSender,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    // Rotate now so the old-epoch send below queues no key update commit.
    bo_group.key_update().await?;
    group.sync().await?;
    match case {
        PastEpochSender::Present => group.update_group_name("renamed".into()).await?,
        PastEpochSender::Removed => group.remove_members(&[bo.inbox_id()]).await?,
        PastEpochSender::LeafReusedBySibling => {
            let bo2 = bo.new_installation().await;
            revoke(&bo, bo.context.installation_id().as_slice()).await;
            // One refresh commit removes bo and adds bo2 into bo's freed leaf.
            group.add_missing_installations().await?;
            let leaf_of = |key: Vec<u8>| {
                group.with_group_snapshot(|mls| {
                    Ok::<_, GroupError>(
                        mls.members()
                            .find(|member| member.signature_key == key)
                            .map(|member| member.index),
                    )
                })
            };
            assert!(leaf_of(bo.context.installation_id().to_vec())?.is_none());
            let bo_leaf =
                bo_group.with_group_snapshot(|mls| Ok::<_, GroupError>(mls.own_leaf_index()))?;
            assert_eq!(
                leaf_of(bo2.context.installation_id().to_vec())?,
                Some(bo_leaf)
            );
        }
    }

    let message_id = bo_group.send_message_optimistic(b"old epoch", SendMessageOpts::default())?;
    bo_group.publish_intents().await?;
    assert!(bo_group.epoch().await? < group.epoch().await?);
    let sent = group.test_get_last_message_from_network().await?;
    assert!(!sent.is_commit());
    group.sync().await?;

    let sender = group
        .find_messages(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.id == message_id)
        .map(|message| message.sender_installation_id);
    if matches!(case, PastEpochSender::Present) {
        assert_eq!(sender, Some(bo.context.installation_id().to_vec()));
    } else {
        assert_eq!(sender, None);
        assert_eq!(group.cursor().await?, sent.cursor);
    }
    Ok(())
}
