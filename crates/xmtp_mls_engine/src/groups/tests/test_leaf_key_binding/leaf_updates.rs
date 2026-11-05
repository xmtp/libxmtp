//! Leaf key binding tests: leaf updates.

use super::*;

/// Bo's identity also associates Alix's installation key, so the key belongs
/// to both inboxes. An update path that moves Alix's leaf to Bo's inbox, in a
/// commit that raises Bo's entry to that identity state, must still be
/// rejected: an update keeps the committer's inbox. The association exists
/// only in Alix's and Bo's local histories.
// verifies: GMOD-043
#[xmtp_common::test(unwrap_try = true)]
async fn update_path_leaf_must_keep_committer_inbox() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let bo_sequence = associate_locally(
        &bo,
        &alix.client.context.identity().installation_keys,
        &[&alix.client, &bo.client],
    )
    .await;
    let bo_inbox_id = bo.client.inbox_id().to_string();
    publish_path_credential(&alix.client, &alix_group, &bo_inbox_id, Some(bo_sequence)).await;

    let alix_key = alix.client.installation_public_key().to_vec();
    assert_rejected(&bo.client, &bo_group, &alix_key, &bo_inbox_id).await;
}

/// Alix commits an update path that rotates Alix's leaf to a key that no
/// identity update associates with Alix's inbox.
// verifies: GMOD-012
#[xmtp_common::test(unwrap_try = true)]
async fn update_path_leaf_key_must_be_registered() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let alix = &alix.client;
    let signer = alix.context.identity().installation_keys.clone();
    let unregistered = XmtpInstallationCredential::new();
    let unregistered_id = unregistered.public_slice().to_vec();
    let credential_with_key = CredentialWithKey {
        credential: create_credential(alix.inbox_id())?,
        signature_key: unregistered_id.clone().into(),
    };
    publish_raw!(alix, alix_group, |mls, provider| {
        let bundle = commit_without_group_info(mls, provider.storage(), |builder| builder)
            .unwrap()
            .build_with_new_signer(
                provider.rand(),
                provider.crypto(),
                &signer,
                NewSignerBundle {
                    signer: &unregistered,
                    credential_with_key,
                },
                |_| true,
            )
            .unwrap()
            .stage_commit(provider)
            .unwrap();
        vec![bundle.into_messages().0]
    });

    assert_rejected(&bo.client, &bo_group, &unregistered_id, alix.inbox_id()).await;
}

/// Bo proposes an Update that moves Bo's leaf to another key of Bo's inbox,
/// and Alix commits it by reference. The key is either that of Bo's second
/// installation, which already has a leaf, or one with no leaf. Both keys
/// belong to Bo's inbox, but an Update must keep its leaf's key.
// verifies: GMOD-003
#[rstest::rstest]
#[case::sibling_key(true)]
#[case::unused_key(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn update_proposal_must_keep_its_leaf_key(
    #[case] sibling: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = member().await;
    let bo = member().await;
    let new_key = if sibling {
        let bo2 = bo.client.new_installation_with(|o| o.triggers = true).await;
        bo2.context.identity().installation_keys.clone()
    } else {
        let key = XmtpInstallationCredential::new();
        associate(&bo, &key).await;
        key
    };
    let new_key_id = new_key.public_slice().to_vec();
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;

    let identity = bo_group.context.identity();
    let proposal = state_write(bo_group.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let new_signer = NewSignerBundle {
                signer: &new_key,
                credential_with_key: CredentialWithKey {
                    credential: identity.credential(),
                    signature_key: new_key_id.clone().into(),
                },
            };
            let (message, _) = mls
                .propose_self_update_with_new_signer(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &identity.installation_keys,
                    new_signer,
                    LeafNodeParameters::default(),
                )
                .unwrap();
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
    bo_group.context.api().send_group_messages(messages).await?;
    alix_group.sync().await?;

    let alix = &alix.client;
    let signer = alix.context.identity().installation_keys.clone();
    publish_raw!(alix, alix_group, |mls, provider| {
        let (upkeep, updates) = membership_upkeep(mls, &[]).unwrap();
        let mut stage = commit_without_group_info(mls, provider.storage(), |builder| {
            builder.consume_proposal_store(true).add_proposals(upkeep)
        })
        .unwrap();
        stage.with_app_data_dictionary_updates(updates);
        let bundle = stage
            .build(provider.rand(), provider.crypto(), &signer, |_| true)
            .unwrap()
            .stage_commit(provider)
            .unwrap();
        vec![bundle.into_messages().0]
    });

    if !sibling {
        assert_rejected(&bo.client, &bo_group, &new_key_id, bo.client.inbox_id()).await;
        return Ok(());
    }
    let _ = bo_group.sync().await;
    let leaves = bo_group.with_group_snapshot(|mls| {
        Ok(mls
            .members()
            .filter(|m| m.signature_key == new_key_id)
            .count())
    })?;
    assert_eq!(leaves, 1, "the sibling key must keep a single leaf");
    let rejection = bo_group
        .context
        .db()
        .read_last_rejection(&StreamTopic::group(bo_group.group_id))?;
    assert_eq!(rejection.unwrap().code, "commit_validation_failure");
    Ok(())
}

/// Bo registers a new installation on Bo's inbox and publishes its key
/// package with a credential that names Alix. A membership refresh must not
/// add that package, which every receiver would reject. It records the
/// installation as failed instead, and the refresh still succeeds.
// verifies: JOIN-093, JOIN-012
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_skips_package_that_names_another_inbox() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let forged_key = XmtpInstallationCredential::new();
    associate(&bo, &forged_key).await;
    let forged_key_id = forged_key.public_slice().to_vec();
    let alix_inbox_id = alix.client.inbox_id().to_string();
    let forged_package = XmtpKeyPackage::builder()
        .inbox_id(alix_inbox_id.clone())
        .credential(create_credential(&alix_inbox_id)?)
        .installation_keys(forged_key)
        .build(&bo.client.context.mls_provider(), false)?
        .key_package;
    bo.client
        .context
        .api()
        .upload_key_package(forged_package.tls_serialize_detached()?)
        .await?;

    alix_group.update_installations().await?;

    assert!(
        membership(&alix_group)
            .await
            .failed_installations
            .contains(&forged_key_id)
    );
    assert!(!has_leaf(&alix_group, &forged_key_id, &alix_inbox_id).await);

    // Bo applies the refresh.
    let bo_sequence_before = membership(&bo_group)
        .await
        .get(bo.client.inbox_id())
        .copied();
    bo_group.sync().await?;
    assert_ne!(
        membership(&bo_group)
            .await
            .get(bo.client.inbox_id())
            .copied(),
        bo_sequence_before
    );
    assert!(
        bo.client
            .context
            .db()
            .read_last_rejection(&StreamTopic::group(bo_group.group_id))?
            .is_none()
    );
}

/// Bo and Mallory, an inbox outside the group, both hold key K. Bo publishes
/// a pending Add of K's package, which names Mallory. Alix's refresh raises
/// Bo's entry, so K is an expected addition under Bo only. The package does
/// not name Bo, so the refresh records K as failed and leaves the pending Add
/// out, instead of selecting a leaf that receivers reject. Bo's association
/// exists only in Alix's local history.
// verifies: JOIN-093
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_omits_pending_add_under_another_owner() {
    let alix = member().await;
    let bo = member().await;
    let mallory = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let shared_key = XmtpInstallationCredential::new();
    let shared_key_id = shared_key.public_slice().to_vec();
    associate(&mallory, &shared_key).await;
    associate_locally(&bo, &shared_key, &[&alix.client]).await;
    let mallory_inbox_id = mallory.client.inbox_id().to_string();
    let package = XmtpKeyPackage::builder()
        .inbox_id(mallory_inbox_id.clone())
        .credential(create_credential(&mallory_inbox_id)?)
        .installation_keys(shared_key)
        .build(&mallory.client.context.mls_provider(), false)?
        .key_package;
    mallory
        .client
        .context
        .api()
        .upload_key_package(package.tls_serialize_detached()?)
        .await?;
    let bo_signer = bo.client.context.identity().installation_keys.clone();
    publish_raw!(bo.client, bo_group, |mls, provider| {
        let (add, _) = mls
            .propose_add_member(provider, &bo_signer, &package)
            .unwrap();
        vec![add]
    });
    // Receive without the refresh that `sync` runs.
    alix_group.receive().await?;
    let pending_adds = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(
                mls_group
                    .pending_proposals()
                    .filter(|proposal| {
                        matches!(proposal.proposal(), openmls::prelude::Proposal::Add(_))
                    })
                    .count(),
            )
        })
        .await?;
    assert_eq!(pending_adds, 1, "Alix must hold Bo's pending Add");

    alix_group.update_installations().await?;

    assert!(
        membership(&alix_group)
            .await
            .failed_installations
            .contains(&shared_key_id)
    );
    assert!(!has_leaf(&alix_group, &shared_key_id, &mallory_inbox_id).await);
    alix_group
        .update_group_name("after the refresh".into())
        .await?;
}
