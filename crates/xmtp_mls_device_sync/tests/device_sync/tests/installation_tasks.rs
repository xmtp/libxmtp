//! Device sync tests: installation tasks.

use super::*;

// verifies: SYNC-014
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_only_added_to_correct_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, stream, sync_worker);
    tester!(bo);

    let old_group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    old_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&old_group.group_id))
            .set((dsl::last_message_ns.eq(0), dsl::created_at_ns.eq(0)))
            .execute(conn)
    })?;

    let bo_group_denied = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_group_unknown = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_dm = bo.find_or_create_dm(alix1.inbox_id(), None).await?;

    alix1.sync_welcomes().await?;
    let alix_bo_group_denied = alix1.group(&bo_group_denied.group_id)?;
    let alix_bo_group_unknown = alix1.group(&bo_group_unknown.group_id)?;
    let alix_bo_dm = alix1.group(&bo_dm.group_id)?;

    alix_bo_dm.update_consent_state(ConsentState::Allowed)?;
    alix_bo_group_denied.update_consent_state(ConsentState::Denied)?;

    let new_group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    new_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;

    tester!(alix2, from: alix1);

    // The adds are durable TaskRunner work now — the metric above fires when
    // the tasks are scheduled, not when the commits land. Poll until alix2
    // has been welcomed into every eligible group: the new fresh group, the
    // unknown-consent group, and the consented DM.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        (alix2.group(&new_group.group_id).is_ok()
            && alix2.group(&alix_bo_group_unknown.group_id).is_ok()
            && alix2.group(&alix_bo_dm.group_id).is_ok())
        .then_some(())
    })
    .await
    .expect("alix2 must be added to all eligible groups");

    // The negatives are meaningful once the positive set has converged: the
    // filtered-out groups never get an AddMissingInstallations task at all.
    // Not added to old stale group
    let alix2_old_group = alix2.group(&old_group.group_id);
    assert!(alix2_old_group.is_err());

    // Not added to denied group from Bo
    let alix2_bo_group_denied = alix2.group(&alix_bo_group_denied.group_id);
    assert!(alix2_bo_group_denied.is_err());
}

/// The periodic membership refresh skips sync groups, so it adds no newer
/// installation to an older one.
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn periodic_refresh_adds_no_installation_to_a_sync_group() {
    tester!(alix1, disable_workers);
    let old = alix1.device_sync_client().get_sync_group().await?;
    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let alix1_sync_groups = alix1.context.db().all_sync_groups()?;
    assert_eq!(alix1_sync_groups.len(), 2);
    for group in &alix1_sync_groups {
        alix1
            .group(&group.id)?
            .maybe_update_installations(Some(0))
            .await?;
    }

    alix2.sync_welcomes().await?;
    let alix2_sync_groups = alix2.context.db().all_sync_groups()?;
    assert_eq!(alix2_sync_groups.len(), 1);
    assert!(alix2_sync_groups.iter().all(|g| g.id != old.group_id));
}

#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_task_runner_adds_new_installation_to_groups() {
    // Live sync worker + live TaskRunner (tester! defaults enable the runner).
    // `stream` keeps alix1 receiving welcomes so the sync-group welcome from
    // alix2's registration reaches alix1's device-sync worker.
    tester!(alix1, stream, sync_worker);
    tester!(bo);

    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    tester!(alix2, from: alix1);

    // The welcome handler enqueues the task; the TaskRunner publishes the
    // membership commit; alix2 then receives the group via welcome. Poll —
    // the whole chain is async.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        alix2.group(&group.group_id).ok().map(|_| ())
    })
    .await
    .expect("alix2 must receive the group via the TaskRunner membership add");
}

// verifies: SYNC-014
#[xmtp_common::timeout(std::time::Duration::from_secs(30))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_welcome_schedules_add_installation_tasks() {
    use xmtp_mls_core::worker::{WorkerConfig, WorkerKind};

    // TaskRunner disabled so enqueued rows are observable (not consumed).
    let mut cfg = WorkerConfig::default();
    cfg.enabled.insert(WorkerKind::TaskRunner, false);
    tester!(alix1, worker_config: cfg);
    tester!(bo);
    tester!(cy);
    tester!(di);

    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let mut expected = vec![group.group_id.to_vec()];
    for (peer, state, contradictory) in [
        (bo.inbox_id(), ConsentState::Allowed, ConsentState::Denied),
        (cy.inbox_id(), ConsentState::Unknown, ConsentState::Denied),
        (di.inbox_id(), ConsentState::Denied, ConsentState::Allowed),
    ] {
        for _ in 0..2 {
            let dm = MlsGroup::create_dm_without_peer(&alix1.context, peer.to_string())?;
            dm.update_consent_state(state)?;
            // DM consent comes from the peer inbox, not either physical group id.
            decide_at(
                &alix1,
                &ConsentKey::group(dm.group_id.as_ref()),
                contradictory,
                now_ns(),
            )?;
            if state != ConsentState::Denied {
                expected.push(dm.group_id.to_vec());
            }
        }
    }
    expected.sort();

    // Call the schedule path directly; the second pass must dedup by payload hash.
    for _ in 0..2 {
        let scheduled = alix1
            .device_sync_client()
            .schedule_add_installations_to_groups()?;
        assert_eq!(scheduled, expected.len());
        let mut add_tasks = add_task_group_ids(&alix1);
        add_tasks.sort();
        assert_eq!(
            add_tasks, expected,
            "exact eligible group IDs, without duplicates"
        );
    }
}

// verifies: SYNC-014
#[rstest::rstest]
#[case::active_winner(false)]
#[case::stale_winner(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_welcome_limits_duplicate_dm_add_installation_tasks(
    #[case] stale_winner: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix, disable_workers);
    tester!(bo);
    let db = alix.context.db();
    let now = now_ns();
    let first_activity = now - NS_IN_DAY * if stale_winner { 91 } else { 1 };
    let mut ids = Vec::new();
    for i in 0..13 {
        let dm = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
        // Only the first group has a message; it wins unless the activity filter drops it.
        db.raw_query(|conn| {
            diesel::update(dsl::groups.find(&dm.group_id))
                .set((
                    dsl::created_at_ns.eq(now + i),
                    dsl::last_message_ns.eq((i == 0).then_some(first_activity)),
                ))
                .execute(conn)
        })?;
        ids.push(dm.group_id.to_vec());
    }
    let mut expected = ids[3..].to_vec();
    // Reserve the highest-ID eligible winner even when it is the oldest group.
    let remaining = if !stale_winner {
        0
    } else if ids[1] == *ids[1..].iter().max().unwrap() {
        1
    } else {
        2
    };
    expected.push(ids[remaining].clone());
    expected.sort();
    let scheduled = alix
        .device_sync_client()
        .schedule_add_installations_to_groups()?;
    assert_eq!(
        scheduled, 11,
        "winner plus ten most recently active duplicates"
    );
    let mut actual = add_task_group_ids(&alix);
    actual.sort();
    assert_eq!(actual, expected);
    Ok(())
}

/// A reconcile task armed after a rate-limited add is due only once the
/// add's delay has passed, so the runner does not resend it at once.
// verifies: RATE-041
#[rstest::rstest]
#[case::new_task(false)]
#[case::pre_armed(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_add_arms_its_reconcile_after_the_delay(
    #[case] pre_armed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, disable_workers);
    let sync = alix.device_sync_client();
    let group = alix.create_group(None, None)?;
    if pre_armed {
        schedule_add_missing_installations(
            &alix.context.db(),
            alix.context.events(),
            group.group_id,
            Some(xmtp_common::time::Duration::from_secs(60)),
            true,
        )?;
    }
    // Longer than a task's default three-day expiry.
    let delay = xmtp_common::time::Duration::from_secs(4 * 24 * 60 * 60);
    let before = xmtp_common::time::now_ns();
    sync.schedule_add_missing_installations_task(group.group_id, Some(delay))?;
    let task = alix
        .context
        .db()
        .get_tasks()?
        .into_iter()
        .find(|task| {
            matches!(
                xmtp_proto::xmtp::mls::database::Task::decode(task.data.as_slice())
                    .ok()
                    .and_then(|task| task.task),
                Some(TaskKindProto::AddMissingInstallations(_))
            )
        })
        .expect("the reconcile task");
    assert!(task.next_attempt_at_ns >= before + delay.as_nanos() as i64);
    assert!(
        task.expires_at_ns > task.next_attempt_at_ns,
        "the task does not expire before it is due"
    );
    Ok(())
}
