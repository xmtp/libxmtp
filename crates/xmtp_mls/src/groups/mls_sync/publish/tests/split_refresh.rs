//! Split membership refresh publication, caller outcomes, and recovery.

use super::faults::{Fault, FaultyApi, faulty_group, group_message, welcome};
use super::send_state_transitions::{UNRESOLVED, intents_in, status};
use super::*;
use crate::groups::group_permissions::PreconfiguredPolicies;
use std::sync::Arc;
use xmtp_db::group_message::DeliveryStatus;

fn last_refresh_attempt<C: XmtpSharedContext>(
    group: &crate::groups::MlsGroup<C>,
) -> crate::groups::mls_sync::publish::PreparedAttempt {
    use crate::groups::mls_sync::publish::PreparedAttempt;
    use xmtp_db::group_intent::{IntentKind, QueryPreparedEnvelope};
    let db = group.context.db();
    let intent = db
        .find_group_intents(
            group.group_id,
            Some(vec![IntentState::Processed]),
            Some(vec![IntentKind::UpdateGroupMembership]),
        )
        .unwrap()
        .into_iter()
        .max_by_key(|intent| (intent.published_in_epoch, intent.id))
        .unwrap();
    PreparedAttempt::decode(&db.prepared_envelopes(intent.id).unwrap().unwrap()).unwrap()
}

struct SplitRefreshFixture {
    limited: crate::utils::ClientTester,
    bo: crate::utils::ClientTester,
    caro: crate::utils::ClientTester,
    new_devices: Vec<crate::utils::ClientTester>,
    group_id: GroupId,
    limit: usize,
}

/// Establish only the size boundary; the target has not refreshed any devices.
async fn split_refresh_fixture(
    extra_member: Option<&str>,
) -> Result<SplitRefreshFixture, GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let mut members = vec![bo.inbox_id(), caro.inbox_id()];
    members.extend(extra_member);
    let policies = || Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let group = alix
        .create_group_with_members(&members, policies(), None)
        .await?;
    let probe = alix
        .create_group_with_members(&members, policies(), None)
        .await?;
    let combined_probe = alix
        .create_group_with_members(&members, policies(), None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    const NEW_DEVICES_PER_INBOX: usize = 8;
    let mut new_devices = Vec::new();
    for _ in 0..NEW_DEVICES_PER_INBOX {
        tester!(device, from: bo, disable_workers);
        new_devices.push(device);
    }
    probe.add_missing_installations().await?;
    let first_size = last_refresh_attempt(&probe).max_envelope_size();
    for _ in 0..NEW_DEVICES_PER_INBOX {
        tester!(device, from: caro, disable_workers);
        new_devices.push(device);
    }
    probe.add_missing_installations().await?;
    const ENCODING_MARGIN: usize = 128;
    let limit = first_size.max(last_refresh_attempt(&probe).max_envelope_size()) + ENCODING_MARGIN;
    combined_probe.add_missing_installations().await?;
    let combined_size = last_refresh_attempt(&combined_probe)
        .envelopes
        .iter()
        .map(Vec::len)
        .max()
        .unwrap();
    assert!(
        combined_size > limit,
        "combined={combined_size}, limit={limit}"
    );
    tester!(limited, from: alix, snapshot: Arc::new(alix.db_snapshot()),
        configured: |config| config.limits.max_envelope_bytes = limit,
        disable_workers);
    Ok(SplitRefreshFixture {
        limited,
        bo,
        caro,
        new_devices,
        group_id: group.group_id,
        limit,
    })
}

// verifies: GMOD-029
#[rstest::rstest]
#[case::implicit_refresh(false)]
#[case::explicit_existing_add(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_refresh_over_the_envelope_limit_is_split(
    #[case] explicit_existing_add: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let SplitRefreshFixture {
        limited,
        bo,
        caro,
        new_devices,
        group_id,
        limit,
    } = split_refresh_fixture(None).await?;
    let limited_group = limited.group(&group_id)?;
    let before = limited_group.epoch().await?;
    if explicit_existing_add {
        limited_group
            .add_members(&[bo.inbox_id(), caro.inbox_id()])
            .await?;
    } else {
        limited_group.add_missing_installations().await?;
    }
    assert_eq!(limited_group.epoch().await?, before + 2);
    {
        use crate::groups::mls_sync::publish::PreparedAttempt;
        use xmtp_db::group_intent::{IntentKind, QueryPreparedEnvelope};
        let db = limited.context.db();
        let before = i64::try_from(before)?;
        let parts: Vec<_> = db
            .find_group_intents(
                group_id,
                Some(vec![IntentState::Processed]),
                Some(vec![IntentKind::UpdateGroupMembership]),
            )?
            .into_iter()
            .filter(|intent| {
                intent
                    .published_in_epoch
                    .is_some_and(|epoch| epoch >= before)
            })
            .collect();
        assert_eq!(parts.len(), 2);
        for part in parts {
            let saved = db.prepared_envelopes(part.id)?.unwrap();
            let attempt = PreparedAttempt::decode(&saved)?;
            assert!(attempt.max_envelope_size() <= limit);
        }
    }
    let authenticator = limited_group.epoch_authenticator().await?;
    let added_ids: Vec<_> = new_devices
        .iter()
        .map(|client| client.installation_id.to_vec())
        .collect();
    for client in [&bo, &caro].into_iter().chain(&new_devices) {
        client.sync_welcomes().await?;
        let peer = client.group(&group_id)?;
        peer.sync().await?;
        assert_eq!(peer.epoch_authenticator().await?, authenticator);
        let leaves = peer.with_group_snapshot(|mls| {
            Ok::<_, GroupError>(
                mls.members()
                    .map(|member| member.signature_key)
                    .collect::<Vec<_>>(),
            )
        })?;
        assert_eq!(leaves.len(), added_ids.len() + 3);
        assert!(added_ids.iter().all(|id| leaves.contains(id)));
    }
    assert!(limited_group.with_group_snapshot(|mls| {
        let leaves: Vec<_> = mls.members().map(|member| member.signature_key).collect();
        Ok::<_, GroupError>(added_ids.iter().all(|id| leaves.contains(id)))
    })?);
    // The original target is complete, including the child part, on return.
    assert!(
        limited
            .context
            .db()
            .find_group_intents(
                group_id,
                Some(vec![
                    IntentState::ToPublish,
                    IntentState::Published,
                    IntentState::Committed
                ]),
                Some(vec![
                    xmtp_db::group_intent::IntentKind::UpdateGroupMembership
                ]),
            )?
            .is_empty()
    );
    Ok(())
}

// verifies: GMOD-029, GMOD-046
#[rstest::rstest]
#[case::implicit_refresh(false)]
#[case::explicit_existing_add(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn an_inbox_over_the_envelope_limit_keeps_its_removes(
    #[case] explicit_existing_add: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use xmtp_mls_common::app_data::{
        component_id::ComponentId, component_source::read_from_app_data_dict,
        components::tls_map_components::GroupMembershipComponent, typed::Component,
    };
    use xmtp_mls_validation::commit::extract_group_membership;
    use xmtp_proto::xmtp::mls::message_contents::{GroupMembershipEntry, group_membership_entry};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(caro2, from: caro, disable_workers);
    crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![caro2.installation_id.to_vec()]),
    );
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let probe = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(false, None);
    assert!(group.with_group_snapshot(|mls| {
        Ok::<_, GroupError>(
            extract_group_membership(mls.extensions())?
                .failed_installations
                .contains(&caro2.installation_id.to_vec()),
        )
    })?);
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    tester!(bo2, from: bo, disable_workers);
    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo.installation_id.to_vec()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    probe.add_missing_installations().await?;
    // Leave room for the removal and failed key, but exclude the measured Add.
    const EXCLUDED_ADD_BYTES: usize = 512;
    let full_size = last_refresh_attempt(&probe)
        .envelopes
        .iter()
        .map(Vec::len)
        .max()
        .unwrap();
    let limit = full_size - EXCLUDED_ADD_BYTES;
    tester!(limited, from: alix, snapshot: Arc::new(alix.db_snapshot()),
        configured: |config| config.limits.max_envelope_bytes = limit,
        disable_workers);
    let limited_group = limited.group(&group.group_id)?;
    let before = limited_group.epoch().await?;
    let requested: Vec<&str> = if explicit_existing_add {
        vec![bo.inbox_id()]
    } else {
        Vec::new()
    };
    let result = limited_group.add_members(requested.as_slice()).await?;
    assert_eq!(
        result.failed_installations,
        vec![bo2.installation_id.to_vec()]
    );
    assert_eq!(limited_group.epoch().await?, before + 1);
    assert!(last_refresh_attempt(&limited_group).max_envelope_size() <= limit);
    let caro_group = caro.group(&group.group_id)?;
    // Receive without the refresh that `sync` runs: it would add Caro2 back.
    caro_group.receive().await?;
    assert_eq!(
        limited_group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    for peer in [&limited_group, &caro_group] {
        peer.with_group_snapshot(|mls| {
            let membership = extract_group_membership(mls.extensions())?;
            assert!(membership.get(bo.inbox_id()).is_some());
            let entries = GroupMembershipComponent::decode_value(
                read_from_app_data_dict(ComponentId::GROUP_MEMBERSHIP, mls).unwrap(),
            )
            .unwrap();
            let bo_inbox = xmtp_mls_common::inbox_id::InboxId::from_hex(bo.inbox_id()).unwrap();
            let entry = GroupMembershipEntry::decode(entries.get(&bo_inbox).unwrap().as_slice())?;
            let Some(group_membership_entry::Version::V1(entry)) = entry.version else {
                panic!("expected Bo's v1 membership entry");
            };
            assert_eq!(
                entry.failed_installations,
                vec![bo2.installation_id.to_vec()]
            );
            let caro_inbox = xmtp_mls_common::inbox_id::InboxId::from_hex(caro.inbox_id()).unwrap();
            let entry = GroupMembershipEntry::decode(entries.get(&caro_inbox).unwrap().as_slice())?;
            let Some(group_membership_entry::Version::V1(entry)) = entry.version else {
                panic!("expected Caro's v1 membership entry");
            };
            assert_eq!(
                entry.failed_installations,
                vec![caro2.installation_id.to_vec()]
            );
            assert!(
                !mls.members()
                    .any(|member| member.signature_key == bo.installation_id.as_slice())
            );
            assert!(
                !mls.members()
                    .any(|member| member.signature_key == bo2.installation_id.as_slice())
            );
            Ok::<_, GroupError>(())
        })?;
    }
    bo_group.sync().await?;
    assert!(!bo_group.is_active()?);
    // An unchanged inbox does not produce a fresh oversized commit forever.
    // The already-associated failed key stays listed rather than being added again.
    limited_group.add_missing_installations().await?;
    assert_eq!(limited_group.epoch().await?, before + 1);
    Ok(())
}

/// Publish the first part of an oversized refresh through `api`, then let Bo
/// complete the rest without sending that part's Welcomes. The original
/// target is then a no-op while its published child still owes Welcomes.
/// Returns the source group, the original target and the child.
async fn owe_a_split_child_welcome(
    limited: &crate::utils::ClientTester,
    bo: &crate::utils::ClientTester,
    group_id: GroupId,
    api: FaultyApi,
) -> (
    MlsGroup<super::faults::FaultyContext>,
    StoredGroupIntent,
    StoredGroupIntent,
) {
    let bo_group = bo.group(&group_id).unwrap();
    let source = faulty_group(limited, &group_id, api).await;
    let root = QueueIntent::update_group_membership()
        .data(source.get_membership_update_intent(&[], &[]).await.unwrap())
        .queue(&source)
        .unwrap();
    source.publish_intents().await.unwrap();
    let child = intents_in(
        &source,
        &[IntentState::Published],
        IntentKind::UpdateGroupMembership,
    )
    .into_iter()
    .find(|intent| intent.id != root.id)
    .expect("a split child was published");

    let child_cursor = source.published_intent_target(child.id).unwrap().unwrap();
    crate::subscriptions::barrier::wait_through(
        &bo_group.context,
        [(Topic::new_group_message(group_id), child_cursor)].into(),
        None,
    )
    .await
    .unwrap();
    bo_group.add_missing_installations().await.unwrap();
    let peer_cursor = bo_group
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(group_id))
        .unwrap()
        .processed;
    crate::subscriptions::barrier::wait_through(
        &source.context,
        [(Topic::new_group_message(group_id), peer_cursor)].into(),
        None,
    )
    .await
    .unwrap();
    (source, root, child)
}

/// A peer can complete the remaining membership while the sender still owes
/// a Welcome from an earlier split part. The original call must wait for it,
/// including when its own remaining membership change becomes a no-op.
// verifies: SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_split_refresh_waits_for_every_welcome() {
    let SplitRefreshFixture {
        limited,
        bo,
        new_devices,
        group_id,
        limit,
        ..
    } = split_refresh_fixture(None).await?;
    // A failure that is not a refusal can follow a stored Welcome, so the
    // batch stays owed through every round of the call.
    let faults = std::iter::repeat_n(
        Fault::Refuse(tonic::Code::PermissionDenied),
        MAX_GROUP_SYNC_RETRIES * 4 + 1,
    );
    let api = FaultyApi::new(&limited, welcome, faults);
    let (source, root, child) = owe_a_split_child_welcome(&limited, &bo, group_id, api).await;
    assert!(
        source.sync_until_intent_resolved(root.id).await.is_err(),
        "the original refresh cannot succeed while its child Welcome fails"
    );
    let unfinished: StoredGroupIntent = source.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(unfinished.state, IntentState::Committed);

    // Restarting with working publication finishes the same durable Welcome work.
    let resumed = faulty_group(&limited, &group_id, FaultyApi::new(&limited, welcome, [])).await;
    resumed.sync_until_intent_resolved(root.id).await?;
    let child_attempt =
        PreparedAttempt::decode(&resumed.context.db().prepared_envelopes(child.id)?.unwrap())?;
    assert!(child_attempt.max_envelope_size() <= limit);
    assert!(intents_in(&resumed, &UNRESOLVED, IntentKind::UpdateGroupMembership).is_empty());
    for client in &new_devices {
        client.sync_welcomes().await?;
        let peer = client.group(&group_id)?;
        peer.sync().await?;
        assert_eq!(
            peer.epoch_authenticator().await?,
            resumed.epoch_authenticator().await?
        );
    }
}

/// A refused Welcome ends the split part's batch in a round of the original
/// call, while no wait reads that part. The original call still reports that
/// its membership change is applied but its Welcomes were not sent.
// verifies: SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn a_refused_split_child_welcome_fails_the_original_call() {
    let SplitRefreshFixture {
        limited,
        bo,
        group_id,
        ..
    } = split_refresh_fixture(None).await?;
    let api = FaultyApi::new(
        &limited,
        welcome,
        [Fault::Refuse(tonic::Code::InvalidArgument)],
    );
    let (source, root, child) = owe_a_split_child_welcome(&limited, &bo, group_id, api).await;
    let result = source.sync_until_intent_resolved(root.id).await;
    assert!(welcomes_unsendable(&result), "{result:?}");
    let ended: StoredGroupIntent = source.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(ended.state, IntentState::FollowUpFailed);
}

/// A split part can end its batch while the wait for another part of the
/// same target fails. The original call must still report that its
/// membership change is applied but its Welcomes were not sent, not the
/// other wait's error.
// verifies: SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn an_ended_split_part_wins_over_a_failed_wait() {
    let SplitRefreshFixture {
        limited, group_id, ..
    } = split_refresh_fixture(None).await?;
    let api = FaultyApi::new(
        &limited,
        welcome,
        [Fault::Refuse(tonic::Code::InvalidArgument)],
    );
    let group = faulty_group(&limited, &group_id, api.clone()).await;
    let root = QueueIntent::update_group_membership()
        .data(group.get_membership_update_intent(&[], &[]).await?)
        .queue(&group)?;
    group.publish_intents().await?;
    let (child, _) = saved_child(&group, &[IntentState::Published]);
    assert!(!group.receive().await?.is_errored());
    // The first part's Welcomes are refused, so its batch ends.
    assert!(welcomes_unsendable(
        &group.publish_required_welcomes().await
    ));
    let ended: StoredGroupIntent = group.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(ended.state, IntentState::FollowUpFailed);
    // The rest of the target can no longer be confirmed by a read.
    api.reject_queries();

    let result = group.sync_until_intent_resolved(root.id).await;
    assert!(welcomes_unsendable(&result), "{result:?}");
}

fn saved_child<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    states: &[IntentState],
) -> (StoredGroupIntent, PreparedAttempt) {
    intents_in(group, states, IntentKind::UpdateGroupMembership)
        .into_iter()
        .find_map(|intent| {
            let encoded = group.context.db().prepared_envelopes(intent.id).unwrap()?;
            let attempt = PreparedAttempt::decode(&encoded).unwrap();
            attempt.parent_intent.map(|_| (intent, attempt))
        })
        .expect("the oversized refresh must create a saved child")
}

// verifies: SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_stored_message_waits_past_a_later_split_child() {
    let SplitRefreshFixture {
        limited, group_id, ..
    } = split_refresh_fixture(None).await?;
    let group = limited.group(&group_id)?;
    let root = QueueIntent::update_group_membership()
        .data(group.get_membership_update_intent(&[], &[]).await?)
        .queue(&group)?;
    let message = group.send_message_optimistic(b"after split refresh", Default::default())?;
    let pending = intents_in(&group, &[IntentState::ToPublish], IntentKind::SendMessage)
        .pop()
        .unwrap();
    group.publish_intents().await?;
    let (child, attempt) = saved_child(&group, &[IntentState::Published]);
    assert_eq!(attempt.parent_intent, Some(root.id));
    assert!(root.id < pending.id && pending.id < child.id);
    group
        .context
        .db()
        .update_installations_time_checked(&group_id)?;

    group.publish_stored_message(&message).await?;
    assert_eq!(
        status(&group, &message),
        DeliveryStatus::Published,
        "publishing a specific stored message must not stop at its later split child"
    );
}

// verifies: SEND-015
#[xmtp_common::test(unwrap_try = true)]
async fn a_split_parent_keeps_its_published_unconfirmed_outcome() {
    let SplitRefreshFixture {
        limited, group_id, ..
    } = split_refresh_fixture(None).await?;
    let api = FaultyApi::new(&limited, group_message, []);
    let group = faulty_group(&limited, &group_id, api.clone()).await;
    let root = QueueIntent::update_group_membership()
        .data(group.get_membership_update_intent(&[], &[]).await?)
        .queue(&group)?;
    group.publish_intents().await?;
    let (child, attempt) = saved_child(&group, &[IntentState::Published]);
    assert_eq!(attempt.parent_intent, Some(root.id));
    assert!(group.published_intent_target(child.id)?.is_some());
    let before = group.context.db().prepared_envelopes(child.id)?.unwrap();
    api.reject_queries();

    let result = group.sync_until_intent_resolved(root.id).await;
    assert!(
        matches!(result, Err(GroupError::PublishedButUnconfirmed { intent_id, .. }) if intent_id == root.id),
        "a receipt for a split child belongs to its original caller: {result:?}"
    );
    let current: StoredGroupIntent = group.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(current.state, IntentState::Published);
    assert_eq!(
        group.context.db().prepared_envelopes(child.id)?.unwrap(),
        before
    );
    assert_eq!(
        api.requests.lock().len(),
        1,
        "the accepted child must not be republished"
    );
}

// verifies: SEND-016
#[rstest::rstest]
#[case::live(false)]
#[case::reopened(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_split_parent_keeps_its_ordered_rejection(
    #[case] reopened: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tester!(drew, disable_workers);
    let SplitRefreshFixture {
        bo,
        group_id,
        limit,
        ..
    } = split_refresh_fixture(Some(drew.inbox_id())).await?;
    tester!(ordinary, from: bo, snapshot: Arc::new(bo.db_snapshot()),
        configured: |config| config.limits.max_envelope_bytes = limit,
        disable_workers);
    let group = ordinary.group(&group_id)?;
    let result = group.remove_members(&[drew.inbox_id()]).await.map(|_| ());
    let (child, attempt) = saved_child(&group, &[IntentState::Error]);
    let root = attempt.parent_intent.unwrap();
    let is_permission = |summary: &crate::groups::summary::SyncSummary| {
        summary.process.errored.iter().any(|(_, error)| {
            matches!(
                error,
                GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions
                ))
            )
        })
    };
    assert!(
        is_permission(&group.rejected_intent_summary(child.id)?),
        "fixture must reach the ordered permission rejection"
    );
    let result = if reopened {
        let reopened = faulty_group(
            &ordinary,
            &group_id,
            FaultyApi::new(&ordinary, group_message, []),
        )
        .await;
        reopened.sync_until_intent_resolved(root).await.map(|_| ())
    } else {
        result
    };
    let Err(GroupError::Sync(summary)) = result else {
        panic!("expected a stable permission rejection, got {result:?}");
    };
    assert!(
        is_permission(&summary),
        "the original caller lost the child's ordered rejection: {summary:?}"
    );
    Ok(())
}

// verifies: SEND-009, SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_refused_split_child_fails_its_parent_and_releases_later_work() {
    let SplitRefreshFixture {
        limited, group_id, ..
    } = split_refresh_fixture(None).await?;
    let api = FaultyApi::new(
        &limited,
        group_message,
        [Fault::Refuse(tonic::Code::InvalidArgument)],
    );
    let group = faulty_group(&limited, &group_id, api.clone()).await;
    assert!(group.add_missing_installations().await.is_err());
    let (child, attempt) = saved_child(&group, &[IntentState::Error]);
    let parent: StoredGroupIntent = group
        .context
        .db()
        .fetch(&attempt.parent_intent.unwrap())?
        .unwrap();
    assert_eq!(parent.state, IntentState::Error);
    assert_ne!(parent.id, child.id);
    assert_eq!(
        intents_in(
            &group,
            &[IntentState::Error],
            IntentKind::UpdateGroupMembership
        )
        .len(),
        2
    );
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::UpdateGroupMembership).is_empty());
    assert_eq!(
        api.requests.lock().len(),
        1,
        "a refused child must not be regenerated"
    );
    group
        .context
        .db()
        .update_installations_time_checked(&group_id)?;
    let message = group
        .send_message(b"after refused refresh", Default::default())
        .await?;
    assert_eq!(status(&group, &message), DeliveryStatus::Published);
}

/// A confirmed split part can still owe Welcomes while the next part has an
/// ambiguous publish. Reading the old receipt must not hide the newer echo.
// verifies: SEND-015, SEND-022
#[xmtp_common::test(unwrap_try = true)]
async fn an_applied_split_child_does_not_hide_an_ambiguous_parent_echo() {
    let SplitRefreshFixture {
        limited, group_id, ..
    } = split_refresh_fixture(None).await?;
    let group = limited.group(&group_id)?;
    let root = QueueIntent::update_group_membership()
        .data(group.get_membership_update_intent(&[], &[]).await?)
        .queue(&group)?;
    group.publish_intents().await?;
    let (child, attempt) = saved_child(&group, &[IntentState::Published]);
    assert_eq!(attempt.parent_intent, Some(root.id));
    let child_cursor = group.published_intent_target(child.id)?.unwrap();
    crate::subscriptions::barrier::wait_through(
        &group.context,
        [(Topic::new_group_message(group_id), child_cursor)].into(),
        None,
    )
    .await?;
    let committed: StoredGroupIntent = group.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(committed.state, IntentState::Committed);
    drop(group);

    // No OUT_OF_RANGE settlement or live subscription can rescue this read.
    // The first request is the remaining parent commit. Every later group or
    // Welcome publish stays ambiguous, while ordered queries remain healthy.
    const LATER_PUBLISH_FAILURES: usize = 100;
    let faults = std::iter::once(Fault::StoreThenUnavailable).chain(std::iter::repeat_n(
        Fault::Refuse(tonic::Code::Unavailable),
        LATER_PUBLISH_FAILURES,
    ));
    let api = FaultyApi::new(
        &limited,
        |payload| group_message(payload) || welcome(payload),
        faults,
    );
    let source = faulty_group(&limited, &group_id, api.clone()).await;
    assert!(source.sync_until_intent_resolved(root.id).await.is_err());
    assert!(
        !api.lost.lock().is_empty(),
        "the parent was stored before UNAVAILABLE"
    );
    let parent: StoredGroupIntent = source.context.db().fetch(&root.id)?.unwrap();
    assert_eq!(
        parent.state,
        IntentState::Committed,
        "the available parent echo must apply while the older child still owes Welcomes"
    );
    let child: StoredGroupIntent = source.context.db().fetch(&child.id)?.unwrap();
    assert_eq!(child.state, IntentState::Committed);
    assert!(parent.sequence_id.unwrap() > child.sequence_id.unwrap());
}

/// Each part split from a removal-only refresh, at every depth, adds no leaf.
// verifies: SYNC-024
#[xmtp_common::test(unwrap_try = true)]
async fn every_split_part_of_a_removal_only_refresh_adds_no_leaf() {
    tester!(alix, disable_workers);
    let mut members = Vec::new();
    let mut new_devices = Vec::new();
    // Four refreshed inboxes split into two, then one.
    for _ in 0..4 {
        tester!(member, disable_workers);
        members.push(member);
    }
    let inboxes: Vec<_> = members.iter().map(|member| member.inbox_id()).collect();
    let group = alix.create_group_with_members(&inboxes, None, None).await?;
    for member in &members {
        tester!(device, from: member, disable_workers);
        new_devices.push(device);
    }
    let mut data = group.get_membership_update_intent(&[], &[]).await?;
    data.removal_only = true;
    let intent = QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let mut requirements =
        group.with_group_snapshot(|mls| PublishRequirements::capture(mls, &intent))?;
    let mut parts = 0;
    while let Some(part) = group.smaller_membership_refresh(&requirements)? {
        let dependencies = group.resolve_publish_dependencies(&part).await?;
        assert!(
            dependencies.changes?.new_key_packages.is_empty(),
            "split part {parts} of a removal-only refresh adds a leaf"
        );
        parts += 1;
        requirements = part;
    }
    assert_eq!(parts, 2);
}
