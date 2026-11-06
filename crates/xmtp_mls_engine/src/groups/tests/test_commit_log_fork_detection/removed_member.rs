//! Commit log fork detection tests: removed member.

use super::*;

/// Regression test for the fork-detection false positive observed in
/// production: a member that is removed from a group — and possibly re-added
/// later — was flagged as forked, even though nothing diverged.
///
/// Why: when a commit removes us, openmls merges only the public diff
/// (`StagedCommitState::PublicState`) — the group context epoch advances and
/// the group becomes inactive, but a removed member cannot derive the new
/// epoch's secrets. The remaining members derive them and publish the real
/// post-commit authenticator to the remote commit log; the removed member's
/// value can never match it.
///
/// This test verifies the fix end to end:
/// 1. The removal is recorded truthfully (`CommitType::RemovedFromGroup`,
///    pre-commit epoch, `applied == last`) instead of claiming the new epoch
///    with a stale authenticator.
/// 2. While removed, fork detection skips the removal row (terminal marker)
///    and reports `Some(false)` based on the consistent earlier history.
/// 3. After re-adding via a new welcome, the `commit_sequence_id == 0` anchor
///    stops comparison at the rejoin boundary and reports `None` (unknown) —
///    never `Some(true)`.

#[xmtp_common::test(unwrap_try = true)]
async fn test_fork_detection_not_triggered_by_removal_and_readd()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Alix creates a group and adds Bo; Bo joins and settles.
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    // Fork checks select only allowed conversations, so allow Bo's copy first.
    bo_group.update_consent_state(xmtp_db::consent_record::ConsentState::Allowed)?;
    bo_group.sync().await?;

    // A normal commit Bo applies as an active member, so the commit log has a
    // genuine consensus row before the removal.
    alix_group
        .update_group_name("before removal".to_string())
        .await?;
    bo_group.sync().await?;

    // Alix removes Bo; Bo processes the commit that removes him.
    alix_group.remove_members(&[bo.inbox_id()]).await?;
    bo_group.sync().await?;

    // The removal is recorded truthfully: RemovedFromGroup, Success, and the
    // PRE-commit epoch + authenticator (a removed member cannot derive the
    // new epoch's secrets).
    let bo_logs = bo.context.db().get_group_logs(&bo_group.group_id)?;
    let bo_removal = bo_logs.last().expect("bo logged the removal commit");
    assert_eq!(
        bo_removal.commit_type,
        Some(CommitType::RemovedFromGroup.to_string())
    );
    assert_eq!(bo_removal.commit_result, CommitResult::Success);
    assert_eq!(
        bo_removal.applied_epoch_authenticator, bo_removal.last_epoch_authenticator,
        "a removed member's merge does not advance the authenticator"
    );
    let bo_pre_removal = &bo_logs[bo_logs.len() - 2];
    assert_eq!(
        bo_removal.applied_epoch_number, bo_pre_removal.applied_epoch_number,
        "the removal row records the pre-commit epoch"
    );

    // The remaining member applied the commit fully: new epoch, new
    // authenticator, different from anything Bo could compute.
    let alix_logs = alix.context.db().get_group_logs(&alix_group.group_id)?;
    let alix_removal = alix_logs
        .iter()
        .find(|l| l.commit_sequence_id == bo_removal.commit_sequence_id)
        .expect("alix logged the removal commit");
    assert_eq!(
        alix_removal.applied_epoch_number,
        bo_removal.applied_epoch_number + 1
    );
    assert_ne!(
        alix_removal.applied_epoch_authenticator,
        bo_removal.applied_epoch_authenticator
    );

    // Simulate Bo downloading remote commit log consensus: matching entries
    // for the commits he applied while active, and the remaining members'
    // (true) authenticator for the commit that removed him.
    let mut log_sequence_id = 1000;
    for log in &bo_logs {
        if log.commit_result != CommitResult::Success || log.commit_sequence_id == 0 {
            continue; // welcome rows are never uploaded
        }
        let (epoch, auth) = if log.commit_sequence_id == bo_removal.commit_sequence_id {
            (
                alix_removal.applied_epoch_number,
                alix_removal.applied_epoch_authenticator.clone(),
            )
        } else {
            (
                log.applied_epoch_number,
                log.applied_epoch_authenticator.clone(),
            )
        };
        NewRemoteCommitLog {
            log_sequence_id,
            group_id: bo_group.group_id,
            commit_sequence_id: log.commit_sequence_id,
            commit_result: CommitResult::Success,
            applied_epoch_number: epoch,
            applied_epoch_authenticator: auth,
            author_installation_key: vec![],
        }
        .store(&bo.context.db())?;
        log_sequence_id += 1;
    }

    // --- Check 1: while removed. ---
    // The removal row is skipped as a terminal marker; the earlier history
    // matches consensus, so Bo is NOT forked.
    let mut worker = CommitLogWorker::new(bo.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let fork_status = results[0]
        .is_forked
        .as_ref()
        .unwrap()
        .get(bo_group.group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status,
        Some(false),
        "a removed member with consistent prior history must not be reported as forked"
    );

    // --- Check 2: after being re-added. ---
    // Bo's original key package was consumed when he first joined and its
    // rotation is only queued (not yet uploaded) at this point — re-adding
    // with the consumed key package would produce a welcome Bo cannot
    // process. Upload a fresh one first, as would have happened naturally by
    // the time of a real-world re-add.
    bo.rotate_and_upload_key_package().await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    bo_group.sync().await?;
    assert!(
        bo_group.is_active()?,
        "bo must have rejoined via the new welcome"
    );

    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let fork_status = results[0]
        .is_forked
        .as_ref()
        .unwrap()
        .get(bo_group.group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status, None,
        "after rejoining via welcome, comparison anchors at the new chain start (unknown)"
    );

    let db_status = bo
        .context
        .db()
        .get_group_commit_log_forked_status(&bo_group.group_id)?;
    assert_eq!(db_status, None, "stored status must not be forked");

    Ok(())
}
