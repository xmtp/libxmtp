//! Leaf key binding tests: leaf additions.

use super::*;

/// A key added outside the identity diff whose inbox did not propose it.
#[derive(Clone, Copy)]
enum OutsideAdd {
    /// Alix adds Bo's leafless key K under Bo's unchanged entry.
    OtherInboxLeafless,
    /// Bo adds key K, associated after Bo's entry, without raising it.
    OwnKeyAfterEntry,
    /// Alix adds a new key of Alix's inbox with a credential that names Bo.
    CredentialNamesAnotherInbox,
}

/// Only the inbox a leaf's credential names may add a key outside the
/// expected additions, and only a key in that inbox's referenced state. The
/// receiver keeps no forged leaf and can still commit.
// verifies: GMOD-010, GMOD-012
#[rstest::rstest]
#[case::other_inbox_leafless(OutsideAdd::OtherInboxLeafless)]
#[case::own_key_after_entry(OutsideAdd::OwnKeyAfterEntry)]
#[case::credential_names_another_inbox(OutsideAdd::CredentialNamesAnotherInbox)]
#[xmtp_common::test(unwrap_try = true)]
async fn add_outside_identity_diff(
    #[case] row: OutsideAdd,
) -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_mls_validation::commit::CommitRuleError;
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let key = XmtpInstallationCredential::new();
    let key_id = key.public_slice().to_vec();
    let bo_inbox_id = bo.client.inbox_id().to_string();
    let ((sender, sender_group), (receiver, receiver_group)) = match row {
        OutsideAdd::OwnKeyAfterEntry => ((&bo, &bo_group), (&alix, &alix_group)),
        _ => ((&alix, &alix_group), (&bo, &bo_group)),
    };
    let mut raise = None;
    match row {
        OutsideAdd::OtherInboxLeafless => {
            // K has no package, so Alix's refresh lists it as failed.
            associate(&bo, &key).await;
            alix_group.update_installations().await?;
            assert!(
                membership(&alix_group)
                    .await
                    .failed_installations
                    .contains(&key_id)
            );
            bo_group.sync().await?;
        }
        OutsideAdd::OwnKeyAfterEntry => associate(&bo, &key).await,
        OutsideAdd::CredentialNamesAnotherInbox => {
            // Alix's entry must reference the identity state that adds K.
            associate(&alix, &key).await;
            alix.client.inbox_state(true).await?;
            raise = Some(
                alix.client
                    .context
                    .db()
                    .get_latest_sequence_id_for_inbox(alix.client.inbox_id())?
                    as u64,
            );
        }
    }

    let sender = &sender.client;
    let package = XmtpKeyPackage::builder()
        .inbox_id(bo_inbox_id.clone())
        .credential(create_credential(&bo_inbox_id)?)
        .installation_keys(key)
        .build(&sender.context.mls_provider(), false)?
        .key_package;
    let before = openmls::group::MlsGroup::load(
        receiver.client.context.mls_storage(),
        &receiver_group.group_id.to_openmls(),
    )?
    .unwrap();
    let signer = sender.context.identity().installation_keys.clone();
    let raw = publish_raw!(sender, sender_group, |mls, provider| {
        // On WASM the raw commit's transaction holds the only connection.
        let db = provider.storage().db();
        let old = extract_group_membership(mls.extensions()).unwrap();
        let mut new = old.clone();
        if let Some(sequence_id) = raise {
            new.add(sender.inbox_id().to_string(), sequence_id);
        }
        let payload = build_group_membership_app_data_payload(&db, mls, &old, &new).unwrap();
        let (add, _) = mls.propose_add_member(provider, &signer, &package).unwrap();
        let (proposals, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            &xmtp_configuration::LimitsConfiguration::default(),
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                AppDataUpdateOperation::Update(payload.into()),
            )],
        )
        .unwrap();
        let mut messages = vec![add];
        messages.extend(proposals);
        messages.push(bundle.into_messages().0);
        messages
    });

    assert_rule_rejection(receiver_group, &before, raw, |rule| match row {
        OutsideAdd::OtherInboxLeafless => {
            matches!(rule, CommitRuleError::UnexpectedInstallationAdded(keys) if *keys == [key_id.clone()])
        }
        _ => matches!(rule, CommitRuleError::InboxValidationFailed(inbox) if *inbox == bo_inbox_id),
    })
    .await;
    assert!(
        !has_leaf(receiver_group, &key_id, &bo_inbox_id).await,
        "the forged leaf must not enter the receiver's tree"
    );
    receiver_group
        .update_group_name("after the rejected commit".into())
        .await?;
    Ok(())
}

/// `client`'s copy of `group`, once its Welcome is imported.
async fn joined(client: &EngineTestPeer, group: &EngineTestGroup) -> EngineTestGroup {
    // A fixed-target sync can return no new groups after the Welcome was imported.
    xmtp_common::wait_for_eq(
        || async {
            client.sync_welcomes().await.unwrap();
            client.group(&group.group_id).is_ok()
        },
        true,
    )
    .await
    .unwrap();
    client.group(&group.group_id).unwrap()
}

/// How Bo2's key comes to have no leaf, and who refreshes after its package
/// is served again.
#[derive(Clone, Copy, PartialEq)]
enum Leafless {
    /// Alix's refresh lists Bo2 as failed, then Bo1 refreshes.
    ListedBoRefreshes,
    /// Alix's refresh lists Bo2 as failed, then Alix refreshes.
    ListedAlixRefreshes,
    /// Caro raises Bo's entry without adding or listing Bo2, which Alix
    /// rejects because it leaves Bo2 unaccounted, then Bo1 refreshes.
    Omitted,
}

/// An installation of Bo's inbox has no leaf, and no identity change makes it
/// an expected addition. Bo1's refresh adds it back and Alix accepts the
/// commit. Alix's refresh does not retry another inbox's key. The floor
/// gate is covered by `retry_waits_for_the_floor`: every new group starts at it.
/// A raise that leaves Bo2 neither added nor listed is rejected, so in that
/// row Bo1's refresh adds Bo2 as a new installation.
// verifies: GMOD-055, GMOD-010, PERM-023, GMOD-057
#[rstest::rstest]
#[case::listed_bo_refreshes(Leafless::ListedBoRefreshes)]
#[case::listed_alix_refreshes(Leafless::ListedAlixRefreshes)]
#[case::omitted(Leafless::Omitted)]
#[xmtp_common::test(unwrap_try = true)]
async fn own_inbox_retries_leafless_key(
    #[case] row: Leafless,
) -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = joined(&bo, &alix_group).await;
    let bo2 = bo.new_installation().await;
    let bo2_id = bo2.context.installation_id().to_vec();
    let bo_inbox_id = bo.inbox_id().to_string();

    let mut rejected_at = None;
    if row == Leafless::Omitted {
        let caro_group = joined(&caro, &alix_group).await;
        caro.inbox_state(true).await?;
        let bo_sequence = caro
            .context
            .db()
            .get_latest_sequence_id_for_inbox(&bo_inbox_id)? as u64;
        let signer = caro.context.identity().installation_keys.clone();
        let caro_client = &caro;
        let raw = publish_raw!(caro_client, caro_group, |mls, provider| {
            let db = provider.storage().db();
            let old = extract_group_membership(mls.extensions()).unwrap();
            let mut new = old.clone();
            new.add(bo_inbox_id.clone(), bo_sequence);
            let payload = build_group_membership_app_data_payload(&db, mls, &old, &new).unwrap();
            let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
                mls,
                provider,
                &signer,
                &[],
                &xmtp_configuration::LimitsConfiguration::default(),
                vec![(
                    ComponentId::GROUP_MEMBERSHIP,
                    AppDataUpdateOperation::Update(payload.into()),
                )],
            )
            .unwrap();
            proposals.push(bundle.into_messages().0);
            proposals
        });
        // Receive without the refresh that `sync` runs on the state before it.
        // One receive can miss a fresh write, so poll.
        xmtp_common::wait_for_eq(
            || async {
                alix_group.receive().await.unwrap();
                alix.context
                    .db()
                    .read_last_rejection(&StreamTopic::group(alix_group.group_id))
                    .unwrap()
                    .map(|rejection| rejection.sequence_id)
            },
            Some(raw.2),
        )
        .await?;
        assert_ne!(
            membership(&alix_group).await.get(&bo_inbox_id).copied(),
            Some(bo_sequence)
        );
        rejected_at = Some(raw.2);
    } else {
        set_test_mode_upload_malformed_keypackage(true, Some(vec![bo2_id.clone()]));
        let refresh = alix_group.update_installations().await;
        set_test_mode_upload_malformed_keypackage(false, None);
        refresh?;
    }
    let listed = row != Leafless::Omitted;
    assert_eq!(
        membership(&alix_group)
            .await
            .failed_installations
            .contains(&bo2_id),
        listed
    );
    assert!(!has_leaf(&alix_group, &bo2_id, &bo_inbox_id).await);

    if row == Leafless::ListedAlixRefreshes {
        alix_group.update_installations().await?;
    } else {
        bo_group.sync().await?;
        bo_group.update_installations().await?;
    }
    alix_group.sync().await?;

    let added = row != Leafless::ListedAlixRefreshes;
    assert_eq!(has_leaf(&alix_group, &bo2_id, &bo_inbox_id).await, added);
    assert_eq!(
        membership(&alix_group)
            .await
            .failed_installations
            .contains(&bo2_id),
        !added
    );
    assert_eq!(
        alix.context
            .db()
            .read_last_rejection(&StreamTopic::group(alix_group.group_id))?
            .map(|rejection| rejection.sequence_id),
        rejected_at
    );
    Ok(())
}
