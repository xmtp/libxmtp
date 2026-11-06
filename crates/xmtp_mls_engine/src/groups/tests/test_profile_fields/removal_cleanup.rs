//! Profile field tests: removal cleanup.

use super::*;

/// A commit that is not a membership change but sweeps pending proposals.
enum Sweep {
    MetadataWrite,
    KeyUpdate,
    PendingProposals,
}

/// Carol, a named admin, is removed by a pending proposal that `sweep`
/// commits. The commit must delete her display name and admin-list entry,
/// for the committer and for a receiver.
async fn assert_sweep_cleans_up(sweep: Sweep) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    let carol_group = carol.sync_welcomes().await?.remove(0);
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    group.sync().await?;
    group
        .update_admin_list(UpdateAdminListType::Add, carol.inbox_id().to_string())
        .await?;
    propose_removal(&group, carol.inbox_id()).await?;

    match sweep {
        Sweep::MetadataWrite => group.update_group_name("Renamed".into()).await?,
        Sweep::KeyUpdate => group.key_update().await?,
        Sweep::PendingProposals => {
            let intent = QueueIntent::commit_pending_proposals().queue(&group)?;
            group.sync_until_intent_resolved(intent.id).await?;
        }
    }
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, group.epoch().await?);
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert!(display_names(member)?.is_empty());
        assert!(admins(member)?.is_empty());
    }
    Ok(())
}

/// The commit that removes a member also deletes its display name and
/// admin-list entry, so no stale profile or role outlives the membership.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_deletes_removed_member_entries() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    for (member, inbox_id, name) in [
        (&bo_group, bo.inbox_id(), b"Bo".as_slice()),
        (&carol_group, carol.inbox_id(), b"Carol".as_slice()),
    ] {
        member.sync().await?;
        write(
            member,
            ComponentId::USER_DISPLAY_NAME,
            insert_name(inbox_id, name),
        )
        .await?;
    }
    group.sync().await?;
    group
        .update_admin_list(UpdateAdminListType::Add, carol.inbox_id().to_string())
        .await?;
    assert_eq!(admins(&group)?, vec![inbox(carol.inbox_id())]);

    let epoch = group.epoch().await?;
    group.remove_members(&[carol.inbox_id()]).await?;
    assert_eq!(group.epoch().await?, epoch + 1);
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(bo.inbox_id()), b"Bo".to_vec())])
        );
        assert!(admins(member)?.is_empty());
    }
}

/// A remover that may not delete an admin-list entry still removes the
/// member: it omits that cleanup, keeps the display-name cleanup, and
/// receivers accept the commit without it.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_omits_forbidden_cleanup() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    for admin in [bo.inbox_id(), carol.inbox_id()] {
        group
            .update_admin_list(UpdateAdminListType::Add, admin.to_string())
            .await?;
    }
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    carol_group.sync().await?;
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    bo_group.sync().await?;

    // Bo is an admin, not a super admin, so the admin list is closed to him.
    let epoch = bo_group.epoch().await?;
    bo_group.remove_members(&[carol.inbox_id()]).await?;
    assert_eq!(bo_group.epoch().await?, epoch + 1);
    group.sync().await?;
    assert!(last_rejection(&group)?.is_none());
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert!(display_names(member)?.is_empty());
        assert!(admins(member)?.contains(&inbox(carol.inbox_id())));
    }
}

/// A metadata write that commits a pending removal also cleans up after
/// the removed member.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_metadata_write_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::MetadataWrite).await?;
}

/// A key update that commits a pending removal also cleans up after the
/// removed member.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_key_update_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::KeyUpdate).await?;
}

/// A commit of pending proposals that removes a member also cleans up
/// after it.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_commit_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::PendingProposals).await?;
}

/// Cleanup is the sender's courtesy: a receiver accepts a removal that
/// omits a delete its sender was authorized to include, as an older
/// client's removal would.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_without_cleanup_is_accepted() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    group.sync().await?;
    propose_removal(&group, carol.inbox_id()).await?;
    bo_group.sync().await?;
    let before = last_rejection(&bo_group)?;
    let epoch = bo_group.epoch().await?;

    publish_unchecked(&group, vec![]).await?;
    bo_group.sync().await?;
    assert_eq!(last_rejection(&bo_group)?, before);
    assert_eq!(bo_group.epoch().await?, epoch + 1);
    assert_eq!(bo_group.members().await?.len(), 2);
    assert_eq!(
        display_names(&bo_group)?,
        BTreeMap::from([(inbox(carol.inbox_id()), b"Carol".to_vec())])
    );
}
