//! Client group tests: dm repair.

use super::*;

/// `peer` joins `group_id` from its Welcome and holds `message` there.
async fn assert_peer_received(
    peer: &crate::utils::FullXmtpClient,
    group_id: &xmtp_proto::types::GroupId,
    message: &[u8],
) {
    let joined = peer.wait_for_welcomes().await.unwrap();
    assert!(joined.iter().any(|group| &group.group_id == group_id));
    let group = peer.group(group_id).unwrap();
    group.sync().await.unwrap();
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert!(
        messages
            .iter()
            .any(|stored| stored.decrypted_message_bytes == message)
    );
}

/// A DM whose creation stopped between storing the group and adding the peer
/// is completed when find-or-create returns it, so the peer joins it.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_creation_is_repaired_on_reuse() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let reused = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_eq!(reused.group_id, orphan.group_id);
    assert!(
        reused
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id())
    );

    let message = b"sent after the reuse";
    reused
        .send_message(message, SendMessageOpts::default())
        .await?;
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// A send on a DM whose creation stopped before the peer was added, reached
/// without find-or-create, adds the peer before it queues the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_creation_is_repaired_on_send() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let message = b"sent on the interrupted DM";
    alix.group(&orphan.group_id)?
        .send_message(message, SendMessageOpts::default())
        .await?;
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// Two repairs of one interrupted DM can run at once. Both succeed, and the
/// peer joins and reads the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_repairs_of_an_interrupted_dm_add_the_peer_once() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let message = b"sent during a second repair";
    let (sent, found) = futures::join!(
        orphan.send_message(message, SendMessageOpts::default()),
        alix.find_or_create_dm(bo.inbox_id(), None),
    );
    sent?;
    assert_eq!(found?.group_id, orphan.group_id);
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// When the peer of an interrupted DM cannot be added, find-or-create and
/// send return the add's failure instead of the DM or the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_whose_peer_cannot_be_added_fails() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    // An inbox with no identity log has no installation to add.
    let peer = hex::encode(xmtp_cryptography::rand::rand_array::<32>());
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, peer.clone())?;

    assert!(alix.find_or_create_dm(peer.as_str(), None).await.is_err());
    let message = b"never queued";
    let sent = alix
        .group(&orphan.group_id)?
        .send_message(message, SendMessageOpts::default())
        .await;
    assert!(sent.is_err());
    let stored = orphan.find_messages(&MsgQueryArgs::default())?;
    assert!(
        !stored
            .iter()
            .any(|stored| stored.decrypted_message_bytes == message)
    );
}

/// An installation its own inbox revoked cannot complete an interrupted DM:
/// the add resolves as a no-op, so find-or-create fails instead of returning
/// a DM without the peer.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_repair_that_adds_no_peer_fails() {
    use crate::groups::MlsGroup;
    use xmtp_id::associations::test_utils::add_wallet_signature;

    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
    let mut revoke = alix2
        .identity_updates()
        .revoke_installations(vec![alix.installation_id.to_vec()])
        .await?;
    add_wallet_signature(&mut revoke, &alix2.builder.owner).await;
    alix2
        .identity_updates()
        .apply_signature_request(revoke)
        .await?;

    let result = alix.find_or_create_dm(bo.inbox_id(), None).await;
    assert!(result.is_err(), "returned a DM without its peer");
    assert!(bo.sync_welcomes().await?.is_empty());
    assert!(bo.db().find_group(&orphan.group_id)?.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_double_dms() {
    tester!(alice);
    tester!(bob);

    let alice_dm = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm
        .send_message(b"Welcome 1", SendMessageOpts::default())
        .await?;

    let bob_dm = bob
        .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
        .await?;

    tester!(alice2, from: alice);
    let alice_dm2 = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm2
        .send_message(b"Welcome 2", SendMessageOpts::default())
        .await?;

    alice_dm.update_installations().await?;
    alice.sync_welcomes().await?;
    bob.sync_welcomes().await?;

    alice_dm
        .send_message(b"Welcome from 1", SendMessageOpts::default())
        .await?;

    // This message will set bob's dm as the primary DM for all clients
    bob_dm
        .send_message(b"Bob says hi 1", SendMessageOpts::default())
        .await?;
    // Alice will sync, pulling in Bob's DM message, which will cause
    // a database trigger to update `last_message_ns`, putting bob's DM to the top.
    alice_dm.sync().await?;

    alice2.sync_welcomes().await?;
    let mut groups = alice2.find_groups(GroupQueryArgs::default())?;

    assert_eq!(groups.len(), 1);
    let group = groups.pop()?;

    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs::default())?;

    assert_eq!(messages.len(), 6);

    // Reload alice's DM. This will load the DM that Bob just created and sent a message on.
    let new_alice_dm = alice.stitched_group(&alice_dm.group_id)?;

    // The group_id should not be what we asked for because it was stitched
    assert_ne!(alice_dm.group_id, new_alice_dm.group_id);
    // They should be the same, due the the message that Bob sent above.
    assert_eq!(new_alice_dm.group_id, bob_dm.group_id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_with_group_resolves_stitched_dm() {
    tester!(alice, disable_workers);
    tester!(bob, disable_workers);

    let alice_dm = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    let old_message_id = alice_dm
        .send_message(b"from alice", SendMessageOpts::default())
        .await?;
    let bob_dm = bob
        .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
        .await?;
    alice.sync_welcomes().await?;
    bob_dm
        .send_message(b"from bob", SendMessageOpts::default())
        .await?;
    alice_dm.sync().await?;

    let (message, group) = alice.message_with_group(&old_message_id).await?.unwrap();
    assert_eq!(message.id, old_message_id);
    assert_eq!(message.group_id, alice_dm.group_id);
    assert_ne!(group.group_id, message.group_id);
    assert_eq!(group.group_id, bob_dm.group_id);
    assert!(
        alice
            .message_with_group(b"unknown message")
            .await?
            .is_none()
    );
}

/// Alice's DM with bob, plus `count` more groups for the same pair that bob
/// created and alice joined. Returns alice's DM and bob's sibling groups.
async fn dm_with_peer_created_siblings(
    alice: &crate::utils::FullXmtpClient,
    bob: &crate::utils::FullXmtpClient,
    count: usize,
) -> (crate::utils::TestMlsGroup, Vec<crate::utils::TestMlsGroup>) {
    let target = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await
        .unwrap();
    bob.sync_welcomes().await.unwrap();
    let mut siblings = Vec::with_capacity(count);
    for _ in 0..count {
        siblings.push(
            bob.create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
                .await
                .unwrap(),
        );
    }
    alice.sync_welcomes().await.unwrap();
    assert_eq!(alice.db().other_dms(&target.group_id).unwrap().len(), count);
    (target, siblings)
}

/// Sends one message in each group and returns `(group id, message id)` pairs.
async fn send_in_each(
    groups: &[crate::utils::TestMlsGroup],
) -> Vec<(xmtp_proto::types::GroupId, Vec<u8>)> {
    let mut sent = Vec::with_capacity(groups.len());
    for group in groups {
        let id = group
            .send_message(b"sibling message", SendMessageOpts::default())
            .await
            .unwrap();
        sent.push((group.group_id, id));
    }
    sent
}

/// A paused sibling of a stitched DM does not stop the DM's sync: the named
/// group and every other sibling sync, and the paused sibling stays held.
// verifies: DMS-019
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_sync_reaches_target_past_failing_sibling() {
    use xmtp_db::group::QueryGroupVersion;

    tester!(alice, disable_workers);
    tester!(bob, disable_workers);
    let (target, siblings_for_bob) = dm_with_peer_created_siblings(&alice, &bob, 3).await;
    let paused = alice.db().other_dms(&target.group_id)?.first()?.id;
    alice.db().set_group_paused(&paused, "999.0.0")?;

    let target_message = bob
        .group(&target.group_id)?
        .send_message(b"target message", SendMessageOpts::default())
        .await?;
    let sibling_messages = send_in_each(&siblings_for_bob).await;

    target.sync().await?;
    let stored = |id: &[u8]| alice.db().get_group_message(id).map(|m| m.is_some());
    assert!(stored(&target_message)?);
    let mut held = None;
    for (group_id, message) in sibling_messages {
        assert_eq!(stored(&message)?, group_id != paused, "{group_id}");
        if group_id == paused {
            held = Some(message);
        }
    }

    // The paused sibling's envelope was held, not skipped: once the group
    // resumes, its sync processes it.
    alice.db().unpause_group(&paused)?;
    alice.group(&paused)?.sync().await?;
    assert!(stored(&held?)?);
}

/// When the named group of a stitched DM fails, its siblings still sync and
/// the call returns the named group's own error.
// verifies: DMS-019
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_sync_reaches_siblings_past_failing_named_group() {
    use crate::groups::GroupError;
    use xmtp_db::group::{GroupMembershipState, QueryGroupVersion};

    tester!(alice, disable_workers);
    tester!(bob, disable_workers);
    let (target, siblings_for_bob) = dm_with_peer_created_siblings(&alice, &bob, 2).await;
    let assert_siblings_stored = |sent: Vec<(xmtp_proto::types::GroupId, Vec<u8>)>| {
        for (group_id, message) in sent {
            assert!(
                alice.db().get_group_message(&message).unwrap().is_some(),
                "{group_id}"
            );
        }
    };

    alice.db().set_group_paused(&target.group_id, "999.0.0")?;
    let sent = send_in_each(&siblings_for_bob).await;
    let error = target.sync().await.unwrap_err();
    assert!(
        matches!(
            &error,
            GroupError::Sync(summary)
                if matches!(summary.other_error(), Some(GroupError::GroupPausedUntilUpdate(_)))
        ),
        "{error:?}"
    );
    assert_siblings_stored(sent);

    alice.db().unpause_group(&target.group_id)?;
    alice
        .db()
        .update_group_membership(target.group_id, GroupMembershipState::Restored)?;
    let sent = send_in_each(&siblings_for_bob).await;
    let error = target.sync().await.unwrap_err();
    assert!(matches!(error, GroupError::GroupInactive), "{error:?}");
    assert_siblings_stored(sent);
}
