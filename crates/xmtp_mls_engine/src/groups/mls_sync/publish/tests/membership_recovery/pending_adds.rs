//! Membership recovery publication tests: pending adds.

use super::*;

#[rstest::rstest]
#[case::only_requested_add(false)]
#[case::independent_pending_add(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn requested_add_cannot_succeed_after_its_last_package_becomes_unusable(
    #[case] independent_add: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    for inbox in std::iter::once(caro.inbox_id()).chain(independent_add.then_some(dave.inbox_id()))
    {
        let proposal = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                vec![inbox.to_string()],
                vec![],
            ))?)
            .queue(&group)?;
        group.sync_until_intent_resolved(proposal.id).await?;
    }
    // The caller's initial fetch succeeds. The publication fetch then loses
    // the only usable package for the requested inbox.
    let request = QueueIntent::update_group_membership()
        .data(
            group
                .get_membership_update_intent(&[caro.inbox_id()], &[])
                .await?,
        )
        .queue(&group)?;
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let crypto_before = group
        .context
        .db()
        .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?;
    xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![caro.context.installation_id().to_vec()]),
    );
    let result = group.publish_intents().await;
    xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage(
        false, None,
    );
    assert!(
        matches!(result, Err(GroupError::InvalidPublicKeys(_))),
        "{result:?}"
    );
    let rejected: StoredGroupIntent = group.context.db().fetch(&request.id)?.unwrap();
    assert_eq!(rejected.state, IntentState::Error);
    assert!(group.context.db().prepared_envelopes(request.id)?.is_none());
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    assert_eq!(
        crypto_before,
        group
            .context
            .db()
            .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?
    );

    let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(commit.id).await?;
    let members = group.members().await?;
    assert!(
        members
            .iter()
            .any(|member| member.inbox_id == caro.inbox_id())
    );
    assert_eq!(
        members
            .iter()
            .any(|member| member.inbox_id == dave.inbox_id()),
        independent_add
    );
    group
        .send_message(
            b"after rejected unusable request",
            SendMessageOpts::default(),
        )
        .await?;
    Ok(())
}

#[rstest::rstest]
#[case::membership_refresh(true)]
#[case::generic_commit(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn revoked_pending_add_does_not_block_publication(
    #[case] refresh_membership: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), eve.inbox_id()],
            None,
            None,
        )
        .await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    caro_group.receive().await?;
    let bo2 = bo.new_installation().await;
    // Refresh leaf changes are inline. Seed the accepted standalone Add
    // explicitly so this test still exercises pending-proposal recovery.
    publish_installation_add(
        &caro_group,
        bo2.context.installation_id().as_slice(),
        &[&group, &caro_group],
    )
    .await?;
    // Caro is not an admin, so receivers reject the removal.
    assert!(caro_group.remove_members(&[eve.inbox_id()]).await.is_err());
    receive_completed_prefix(&caro_group, &group).await?;
    group.with_group_snapshot(|mls| {
        assert!(mls.pending_proposals().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(add)
                if add.key_package().leaf_node().signature_key().as_slice()
                    == bo2.context.installation_id().as_slice())
        }));
        Ok(())
    })?;
    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo2.context.installation_id().to_vec()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;

    // The installation diff no longer permits Bo2. Preparation must not reuse
    // the accepted Add from the rejected operation after this revocation.
    let kind = if refresh_membership {
        IntentKind::UpdateGroupMembership
    } else {
        IntentKind::CommitPendingProposals
    };
    let refresh = if refresh_membership {
        QueueIntent::update_group_membership()
            .data(group.get_membership_update_intent(&[], &[]).await?)
            .queue(&group)?
    } else {
        QueueIntent::commit_pending_proposals()
            .data(Vec::<u8>::from(CommitPendingProposalsIntentData::new()))
            .queue(&group)?
    };
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let (_, prepared) = prepare_kind(&group, kind).await?;
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    let intent: StoredGroupIntent = group.context.db().fetch(&refresh.id)?.unwrap();
    let commit = decode_staged_commit(intent.staged_commit.as_ref().unwrap())?;
    assert!(
        !commit.add_proposals().any(|add| add
            .add_proposal()
            .key_package()
            .leaf_node()
            .signature_key()
            .as_slice()
            == bo2.context.installation_id().as_slice()),
        "prepared commit includes a revoked installation"
    );
    group.publish_intents().await?;
    group.sync_until_intent_resolved(refresh.id).await?;
    group
        .send_message(b"after pending add revocation", SendMessageOpts::default())
        .await?;
    assert!(!prepared.envelopes.is_empty());
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_after_rejected_membership_refreshes_pending_adds_immediately() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), eve.inbox_id()],
            None,
            None,
        )
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    caro_group.receive().await?;
    let bo2 = bo.new_installation().await;
    // Refresh leaf changes are inline. Seed the accepted standalone Add
    // explicitly so this test still exercises pending-proposal recovery.
    publish_installation_add(
        &caro_group,
        bo2.context.installation_id().as_slice(),
        &[&group, &caro_group],
    )
    .await?;
    // Caro is not an admin, so receivers reject the removal.
    assert!(caro_group.remove_members(&[eve.inbox_id()]).await.is_err());
    receive_completed_prefix(&caro_group, &group).await?;
    // A successful recent check must not hide a valid pending installation Add.
    group
        .context
        .db()
        .update_installations_time_checked(&group.group_id)?;
    group
        .send_message(
            b"immediate send after rejection",
            SendMessageOpts::default(),
        )
        .await?;
    bo2.sync_welcomes().await?;
    let bo2_group = bo2.group(&group.group_id)?;
    bo2_group.receive().await?;
    assert!(
        bo2_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| message.decrypted_message_bytes == b"immediate send after rejection")
    );
}

// verifies: GMOD-038, JOIN-008
#[xmtp_common::test(unwrap_try = true)]
async fn accepted_add_that_expires_before_commit_does_not_block_publication()
-> Result<(), Box<dyn std::error::Error>> {
    const PACKAGE_LIFETIME_SECS: u64 = 10;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id))
    })
    .await
    .expect("Bo must process the initial Welcome");
    let package = xmtp_mls_core::state_tx::state_write(caro.context.mls_storage(), |tx| {
        let storage = tx.storage();
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let now = u64::try_from(xmtp_common::time::now_secs()).unwrap();
        let generated = xmtp_id::key_package::build_key_package(
            caro.inbox_id(),
            caro.context.identity().credential(),
            &caro.context.identity().installation_keys,
            &provider,
            xmtp_id::key_package::KeyPackageOptions {
                lifetime: Some(openmls::prelude::Lifetime::init(
                    now,
                    now + PACKAGE_LIFETIME_SECS,
                )),
                ..Default::default()
            },
        )
        .expect("build a package with a bounded lifetime");
        Ok::<_, GroupError>(Continue(generated.bundle.key_package().clone()))
    })?
    .into_continued();
    let bytes = package.tls_serialize_detached()?;
    caro.context.api().upload_key_package(bytes.clone()).await?;
    xmtp_common::wait_for_eq(
        || async {
            let fetched = group
                .context
                .api()
                .fetch_key_packages(&[caro.context.installation_id()])
                .await
                .expect("fetch the short-lived package");
            fetched
                .get(&caro.context.installation_id())
                .and_then(Option::as_ref)
                .map(|entry| entry.key_package_tls_serialized.clone())
        },
        Some(bytes.clone()),
    )
    .await
    .expect("the short-lived package must be readable before proposing it");
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    bo_group.with_group_snapshot(|mls| {
        assert!(mls.pending_proposals().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(add) if add.key_package() == &package)
        }));
        Ok(())
    })?;
    let crypto = openmls_rust_crypto::RustCrypto::default();
    xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(&crypto, &bytes)
        .expect("the received Add must be accepted before its package expires");
    // Wait for the actual validity transition. No clock or receiver policy is changed.
    xmtp_common::wait_for_some(|| async {
        xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(&crypto, &bytes)
            .is_err()
            .then_some(())
    })
    .await
    .expect("the accepted package must expire before commit preparation");
    let before = bo_group.with_group_snapshot(PreparedBase::capture)?;
    let commit = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    prepare_kind(&bo_group, IntentKind::CommitPendingProposals).await?;
    assert_eq!(before, bo_group.with_group_snapshot(PreparedBase::capture)?);
    let prepared: StoredGroupIntent = bo_group.context.db().fetch(&commit.id)?.unwrap();
    let staged = decode_staged_commit(prepared.staged_commit.as_deref().unwrap())?;
    assert_eq!(staged.add_proposals().count(), 0);
    assert!(prepared.post_commit_data.is_none());
    bo_group.sync_until_intent_resolved(commit.id).await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert_eq!(group.members().await?.len(), 2);
    assert_eq!(
        group.epoch_authenticator().await?,
        bo_group.epoch_authenticator().await?
    );
    for peer in [&group, &bo_group] {
        peer.with_group_snapshot(|mls| {
            assert_eq!(mls.pending_proposals().count(), 0);
            Ok(())
        })?;
    }
    bo_group
        .send_message(b"after expired Add", SendMessageOpts::default())
        .await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert!(
        group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| { message.decrypted_message_bytes == b"after expired Add" })
    );
    Ok(())
}
