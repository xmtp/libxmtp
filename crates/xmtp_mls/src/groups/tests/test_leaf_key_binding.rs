//! A commit may only add or replace a leaf whose signature key belongs to the
//! inbox its credential names. Otherwise a member that holds one key could
//! place a leaf under another member's inbox and then send messages and
//! proposals as that member.

use crate::{
    builder::ClientBuilder,
    context::XmtpSharedContext,
    groups::{
        GroupError,
        app_data::{membership_upkeep::membership_upkeep, stage_app_data_proposals_and_commit},
        mls_ext::commit_without_group_info,
        mls_sync::{
            generate_prepared_commit,
            update_group_membership::build_group_membership_app_data_payload,
        },
    },
    identity::{XmtpKeyPackage, create_credential},
    state_tx::state_write,
    utils::{FullXmtpClient, TestMlsGroup},
};
use openmls::{
    messages::proposals::AppDataUpdateOperation,
    prelude::{
        CredentialWithKey, LeafNodeParameters, MlsMessageOut, NewSignerBundle, tls_codec::Serialize,
    },
};
use openmls_traits::OpenMlsProvider;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential, utils::generate_local_wallet};
use xmtp_db::{
    TransactionOutcome::Continue,
    XmtpOpenMlsProviderRef,
    identity_update::QueryIdentityUpdates,
    incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
};
use xmtp_id::associations::{
    InstallationKeyContext, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_wallet_signature},
    unverified::UnverifiedSignature,
};
use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_mls_validation::{
    commit::{extract_group_membership, inbox_id_from_credential},
    group_membership::GroupMembership,
};

type LocalWallet = alloy::signers::local::PrivateKeySigner;

/// Build messages on the sender's current epoch with `$build`, without
/// merging the commit, and publish them in one request.
macro_rules! publish_raw {
    ($sender:expr, $group:expr, |$mls:ident, $provider:ident| $build:block) => {{
        let storage = $sender.context.mls_storage();
        let (payloads, staged_commit, committer) = state_write(storage, |tx| {
            tx.with_group($group.group_id, |mls_group, storage| {
                let (messages, staged_commit, _epoch) =
                    generate_prepared_commit(storage, mls_group, |$mls, $provider| {
                        let messages: Vec<MlsMessageOut> = $build;
                        Ok::<_, GroupError>(messages)
                    })?;
                let payloads = messages
                    .iter()
                    .map(Serialize::tls_serialize_detached)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, GroupError>(Continue((
                    payloads,
                    staged_commit,
                    mls_group.own_leaf_index(),
                )))
            })
        })
        .unwrap()
        .into_continued();
        let messages = $group
            .prepare_group_messages(
                payloads
                    .iter()
                    .map(|payload| (payload.as_slice(), false))
                    .collect(),
            )
            .unwrap();
        let receipts = $sender
            .context
            .api()
            .send_group_messages(messages)
            .await
            .unwrap();
        let (_, cursor, _) = xmtp_api_backend::envelope::metadata(
            receipts.last().unwrap(),
            xmtp_proto::types::Topic::new_group_message($group.group_id).kind(),
        )
        .unwrap();
        (staged_commit, committer, cursor)
    }};
}

struct Member {
    client: FullXmtpClient,
    wallet: LocalWallet,
}

async fn member() -> Member {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    Member { client, wallet }
}

/// Alix creates a group with Bo, and Bo joins it.
async fn group(alix: &FullXmtpClient, bo: &FullXmtpClient) -> (TestMlsGroup, TestMlsGroup) {
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .unwrap();
    // A fixed-target sync can return no new groups after the Welcome was imported.
    xmtp_common::wait_for_eq(
        || async {
            bo.sync_welcomes().await.unwrap();
            bo.group(&alix_group.group_id)
                .is_ok_and(|group| group.is_active().unwrap())
        },
        true,
    )
    .await
    .unwrap();
    let bo_group = bo.group(&alix_group.group_id).unwrap();
    bo_group.sync().await.unwrap();
    (alix_group, bo_group)
}

/// Associate `installation` with the member's inbox. The caller holds the
/// installation's private key.
async fn associate(member: &Member, installation: &XmtpInstallationCredential) {
    let client = &member.client;
    let mut request = SignatureRequestBuilder::new(client.inbox_id())
        .add_association(
            MemberIdentifier::installation(installation.public_slice().to_vec()),
            member.wallet.identifier().into(),
        )
        .build();
    let signature = installation
        .credential_sign::<InstallationKeyContext>(request.signature_text())
        .unwrap();
    request
        .add_signature(
            UnverifiedSignature::new_installation_key(signature, installation.verifying_key()),
            &client.context.scw_verifier(),
        )
        .await
        .unwrap();
    add_wallet_signature(&mut request, &member.wallet).await;
    client
        .identity_updates()
        .apply_signature_request(request)
        .await
        .unwrap();
}

/// The group's committed membership component.
async fn membership(group: &TestMlsGroup) -> GroupMembership {
    group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions()).unwrap())
        })
        .await
        .unwrap()
}

/// Whether the group's tree holds a leaf with `key` whose credential names `inbox_id`.
async fn has_leaf(group: &TestMlsGroup, key: &[u8], inbox_id: &str) -> bool {
    group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(mls_group.members().any(|member| {
                member.signature_key == key
                    && inbox_id_from_credential(&member.credential).is_ok_and(|id| id == inbox_id)
            }))
        })
        .await
        .unwrap()
}

/// Assert that the receiver rejected the forged commit, kept no forged leaf,
/// and can still commit afterwards.
async fn assert_rejected(
    receiver: &FullXmtpClient,
    group: &TestMlsGroup,
    key: &[u8],
    inbox_id: &str,
) {
    let _ = group.sync().await;
    assert!(
        !has_leaf(group, key, inbox_id).await,
        "the forged leaf must not enter the receiver's tree"
    );
    let rejection = receiver
        .context
        .db()
        .read_last_rejection(&StreamTopic::group(group.group_id))
        .unwrap()
        .expect("the commit must be recorded as a terminal rejection");
    assert_eq!(rejection.code, "commit_validation_failure");
    // The rejected commit leaves no pending head and no proposal that breaks
    // the receiver's next commit.
    group
        .update_group_name("after the rejected commit".into())
        .await
        .unwrap();
}

/// Publish a commit whose update path leaf keeps Alix's key but names `inbox_id`.
/// With `raise`, the commit also sets that inbox's membership entry to that sequence id.
async fn publish_path_credential(
    alix: &FullXmtpClient,
    group: &TestMlsGroup,
    inbox_id: &str,
    raise: Option<u64>,
) {
    let signer = alix.context.identity().installation_keys.clone();
    let forged = CredentialWithKey {
        credential: create_credential(inbox_id).unwrap(),
        signature_key: signer.public_slice().to_vec().into(),
    };
    let db = alix.context.db();
    publish_raw!(alix, group, |mls, provider| {
        let mut messages = Vec::new();
        if let Some(sequence_id) = raise {
            let old = extract_group_membership(mls.extensions()).unwrap();
            let mut new = old.clone();
            new.add(inbox_id.to_string(), sequence_id);
            let payload = build_group_membership_app_data_payload(&db, mls, &old, &new).unwrap();
            let (proposal, _) = mls
                .propose_app_data_update(
                    provider,
                    &signer,
                    ComponentId::GROUP_MEMBERSHIP.as_u16(),
                    AppDataUpdateOperation::Update(payload.into()),
                )
                .unwrap();
            messages.push(proposal);
        }
        let (upkeep, updates) = membership_upkeep(mls, &[]).unwrap();
        let mut stage = commit_without_group_info(mls, provider.storage(), |builder| {
            builder
                .consume_proposal_store(true)
                .add_proposals(upkeep)
                .force_self_update(true)
                .leaf_node_parameters(
                    LeafNodeParameters::builder()
                        .with_credential_with_key(forged)
                        .build(),
                )
        })
        .unwrap();
        stage.with_app_data_dictionary_updates(updates);
        let bundle = stage
            .build(provider.rand(), provider.crypto(), &signer, |_| true)
            .unwrap()
            .stage_commit(provider)
            .unwrap();
        messages.push(bundle.into_messages().0);
        messages
    });
}

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
    let db = sender.context.db();
    let raw = publish_raw!(sender, sender_group, |mls, provider| {
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
async fn joined(client: &FullXmtpClient, group: &TestMlsGroup) -> TestMlsGroup {
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
    /// Caro raises Bo's entry without adding or listing Bo2, then Bo1 refreshes.
    Omitted,
}

/// An installation of Bo's inbox has no leaf, and no identity change makes it
/// an expected addition. Bo1's refresh adds it back and Alix accepts the
/// commit. Alix's refresh does not retry another inbox's key. The floor
/// gate is covered by `retry_waits_for_the_floor`: every new group starts at it.
// verifies: GMOD-055, GMOD-010, PERM-023
#[rstest::rstest]
#[case::listed_bo_refreshes(Leafless::ListedBoRefreshes)]
#[case::listed_alix_refreshes(Leafless::ListedAlixRefreshes)]
#[case::omitted(Leafless::Omitted)]
#[xmtp_common::test(unwrap_try = true)]
async fn own_inbox_retries_leafless_key(
    #[case] row: Leafless,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::{tester, utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = joined(&bo, &alix_group).await;
    tester!(bo2, from: bo, disable_workers);
    let bo2_id = bo2.context.installation_id().to_vec();
    let bo_inbox_id = bo.inbox_id().to_string();

    if row == Leafless::Omitted {
        let caro_group = joined(&caro, &alix_group).await;
        caro.inbox_state(true).await?;
        let bo_sequence = caro
            .context
            .db()
            .get_latest_sequence_id_for_inbox(&bo_inbox_id)? as u64;
        let signer = caro.context.identity().installation_keys.clone();
        let db = caro.context.db();
        let caro_client = &caro.client;
        publish_raw!(caro_client, caro_group, |mls, provider| {
            let old = extract_group_membership(mls.extensions()).unwrap();
            let mut new = old.clone();
            new.add(bo_inbox_id.clone(), bo_sequence);
            let payload = build_group_membership_app_data_payload(&db, mls, &old, &new).unwrap();
            let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
                mls,
                provider,
                &signer,
                &[],
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
                membership(&alix_group).await.get(&bo_inbox_id).copied()
            },
            Some(bo_sequence),
        )
        .await?;
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
    assert!(
        alix.context
            .db()
            .read_last_rejection(&StreamTopic::group(alix_group.group_id))?
            .is_none()
    );
    Ok(())
}

/// Bo's identity also associates Alix's installation key, so the key belongs
/// to both inboxes. An update path that moves Alix's leaf to Bo's inbox, in a
/// commit that raises Bo's entry to that identity state, must still be
/// rejected: an update keeps the committer's inbox.
// verifies: GMOD-043
#[xmtp_common::test(unwrap_try = true)]
async fn update_path_leaf_must_keep_committer_inbox() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    associate(&bo, &alix.client.context.identity().installation_keys).await;
    let bo_inbox_id = bo.client.inbox_id().to_string();
    let bo_sequence = bo
        .client
        .context
        .db()
        .get_latest_sequence_id_for_inbox(&bo_inbox_id)? as u64;
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
        let bo2 = ClientBuilder::new_test_client(&bo.wallet).await;
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
/// out, instead of selecting a leaf that receivers reject.
// verifies: JOIN-093
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_omits_pending_add_under_another_owner() {
    let alix = member().await;
    let bo = member().await;
    let mallory = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let shared_key = XmtpInstallationCredential::new();
    let shared_key_id = shared_key.public_slice().to_vec();
    associate(&bo, &shared_key).await;
    associate(&mallory, &shared_key).await;
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

/// Check the receiver's exact rule error, then its durable network rejection.
async fn assert_rule_rejection(
    group: &TestMlsGroup,
    before: &openmls::group::MlsGroup,
    raw: (
        Option<Vec<u8>>,
        openmls::prelude::LeafNodeIndex,
        xmtp_proto::types::Cursor,
    ),
    matches_rule: impl Fn(&xmtp_mls_validation::commit::CommitRuleError) -> bool,
) {
    use crate::groups::validated_commit::{CommitValidationError, ValidatedCommit};
    let staged = xmtp_db::db_deserialize(&raw.0.expect("the raw update carries a commit")).unwrap();
    let error = ValidatedCommit::from_staged_commit(&group.context, &staged, raw.1, before)
        .await
        .expect_err("the raw commit must be rejected");
    assert!(
        matches!(&error, CommitValidationError::Rule(rule) if matches_rule(rule)),
        "{error:?}"
    );
    crate::subscriptions::barrier::wait_through(
        &group.context,
        [(
            xmtp_proto::types::Topic::new_group_message(group.group_id),
            raw.2,
        )]
        .into(),
        None,
    )
    .await
    .unwrap();
    let rejection = group
        .context
        .db()
        .read_last_rejection(&StreamTopic::group(group.group_id))
        .unwrap()
        .expect("the network commit must be terminally rejected");
    assert_eq!(rejection.sequence_id, raw.2);
    assert_eq!(rejection.code, "commit_validation_failure");
}

// verifies: GMOD-049
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_rejects_misattributed_failed_installation() {
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{
        app_data::{components::tls_map_components::GroupMembershipComponent, typed::Component},
        inbox_id::InboxId,
        tls_map::TlsMapDelta,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let absent = XmtpInstallationCredential::new();
    associate(&bo, &absent).await;
    let absent_id = absent.public_slice().to_vec();
    bo.client.inbox_state(true).await?;
    let bo_sequence = bo
        .client
        .context
        .db()
        .get_latest_sequence_id_for_inbox(bo.client.inbox_id())? as u64;
    let before = membership(&bo_group).await;
    let receiver_group = openmls::group::MlsGroup::load(
        bo.client.context.mls_storage(),
        &bo_group.group_id.to_openmls(),
    )?
    .unwrap();
    let alix_client = &alix.client;
    let signer = alix_client.context.identity().installation_keys.clone();
    let raw = publish_raw!(alix_client, alix_group, |mls, provider| {
        let old = extract_group_membership(mls.extensions()).unwrap();
        assert!(bo_sequence > *old.get(bo.client.inbox_id()).unwrap());
        let entry = |sequence_id, failed_installations| GroupMembershipEntry {
            version: Some(Version::V1(V1 {
                sequence_id,
                failed_installations,
            })),
        };
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(
                InboxId::from_hex(alix_client.inbox_id()).unwrap(),
                VLBytes::new(
                    entry(
                        *old.get(alix_client.inbox_id()).unwrap(),
                        vec![absent_id.clone()],
                    )
                    .encode_to_vec(),
                ),
            )
            .update(
                InboxId::from_hex(bo.client.inbox_id()).unwrap(),
                VLBytes::new(entry(bo_sequence, vec![]).encode_to_vec()),
            );
        let payload = GroupMembershipComponent::encode_mutation(&delta).unwrap();
        let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                AppDataUpdateOperation::Update(payload.into()),
            )],
        )
        .unwrap();
        let (commit, _welcome, _) = bundle.into_messages();
        proposals.push(commit);
        proposals
    });

    assert_rule_rejection(&bo_group, &receiver_group, raw, |rule| {
        matches!(rule,
            xmtp_mls_validation::commit::CommitRuleError::InboxValidationFailed(inbox)
                if inbox == alix_client.inbox_id()
        )
    })
    .await;
    assert_eq!(membership(&bo_group).await, before);
}

// verifies: GMOD-046, PERM-030
#[xmtp_common::test(unwrap_try = true)]
async fn dictionary_delete_cannot_hide_a_live_leaf() {
    use tls_codec::VLBytes;
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};

    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let before = membership(&bo_group).await;
    let receiver_group = openmls::group::MlsGroup::load(
        bo.client.context.mls_storage(),
        &bo_group.group_id.to_openmls(),
    )?
    .unwrap();
    let bo_id = bo.client.context.installation_id().to_vec();
    let alix_client = &alix.client;
    let signer = alix_client.context.identity().installation_keys.clone();
    let raw = publish_raw!(alix_client, alix_group, |mls, provider| {
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .delete(InboxId::from_hex(bo.client.inbox_id()).unwrap());
        let payload = delta.tls_serialize_detached().unwrap();
        let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                AppDataUpdateOperation::Update(payload.into()),
            )],
        )
        .unwrap();
        proposals.push(bundle.into_messages().0);
        proposals
    });
    assert_rule_rejection(&bo_group, &receiver_group, raw, |rule| {
        matches!(
            rule,
            xmtp_mls_validation::commit::CommitRuleError::UnexpectedInstallationsRemoved(keys) if keys.contains(&bo_id)
        )
    })
    .await;
    assert_eq!(membership(&bo_group).await, before);
    assert!(has_leaf(&bo_group, &bo_id, bo.client.inbox_id()).await);
}

// verifies: GMOD-047, GMOD-049
#[rstest::rstest]
#[case::omit_only(false)]
#[case::propose_correct_request(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn misattributed_pending_delta_is_omitted(
    #[case] propose_correct_request: bool,
) -> Result<(), GroupError> {
    use crate::{
        groups::intents::{ProposeMemberUpdateIntentData, QueueIntent},
        mls_store::MlsStore,
    };
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    let alix = member().await;
    let bo = member().await;
    let caro = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let absent = XmtpInstallationCredential::new();
    associate(&caro, &absent).await;
    caro.client.inbox_state(true).await?;
    let caro_sequence = caro
        .client
        .context
        .db()
        .get_latest_sequence_id_for_inbox(caro.client.inbox_id())? as u64;
    let caro_id = caro.client.context.installation_id().to_vec();
    let package = MlsStore::new(alix.client.context.clone())
        .get_key_packages_for_installation_ids(vec![caro_id.clone()])
        .await?
        .remove(&caro_id)
        .expect("Caro's primary has a key package")
        .expect("Caro's package verifies")
        .inner;
    let before = membership(&bo_group).await;
    let entry = |sequence_id, failed_installations| GroupMembershipEntry {
        version: Some(Version::V1(V1 {
            sequence_id,
            failed_installations,
        })),
    };
    let delta = TlsMapDelta::<InboxId, VLBytes>::new()
        .update(
            InboxId::from_hex(alix.client.inbox_id()).unwrap(),
            VLBytes::new(
                entry(
                    *before.get(alix.client.inbox_id()).unwrap(),
                    vec![absent.public_slice().to_vec()],
                )
                .encode_to_vec(),
            ),
        )
        .insert(
            InboxId::from_hex(caro.client.inbox_id()).unwrap(),
            VLBytes::new(entry(caro_sequence, vec![]).encode_to_vec()),
        );
    let bad_payload = delta.tls_serialize_detached()?;
    let alix_client = &alix.client;
    let signer = alix_client.context.identity().installation_keys.clone();
    let raw = publish_raw!(alix_client, alix_group, |mls, provider| {
        let (add, _) = mls.propose_add_member(provider, &signer, &package).unwrap();
        let (proposal, _) = mls
            .propose_app_data_update(
                provider,
                &signer,
                ComponentId::GROUP_MEMBERSHIP.as_u16(),
                AppDataUpdateOperation::Update(bad_payload.clone().into()),
            )
            .unwrap();
        vec![add, proposal]
    });
    crate::subscriptions::barrier::wait_through(
        &bo_group.context,
        [(
            xmtp_proto::types::Topic::new_group_message(bo_group.group_id),
            raw.2,
        )]
        .into(),
        None,
    )
    .await?;
    bo_group.load_mls_group_with_lock_async(async |mls| {
        assert!(mls.pending_proposals().any(|queued| {
            matches!(queued.proposal(), openmls::prelude::Proposal::AppDataUpdate(update)
                if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16()
                    && matches!(update.operation(), AppDataUpdateOperation::Update(bytes) if bytes.as_slice() == bad_payload))
        }), "the malformed membership delta must reach selection");
        assert!(mls.pending_proposals().any(|queued| matches!(queued.proposal(), openmls::prelude::Proposal::Add(add) if add.key_package() == &package)));
        Ok::<_, GroupError>(())
    }).await?;
    if propose_correct_request {
        let intent = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                vec![caro.client.inbox_id().to_string()],
                vec![],
            ))?)
            .queue(&bo_group)?;
        bo_group.sync_until_intent_resolved(intent.id).await?;
        let pending_memberships = bo_group
            .load_mls_group_with_lock_async(async |mls| {
                Ok::<_, GroupError>(mls.pending_proposals().filter(|queued| {
                matches!(queued.proposal(), openmls::prelude::Proposal::AppDataUpdate(update)
                    if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16())
            }).count())
            })
            .await?;
        assert_eq!(
            pending_memberships, 2,
            "the correct request must publish its own membership delta"
        );
    }
    let intent = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(intent.id).await?;
    if propose_correct_request {
        let (membership, failed) = bo_group
            .load_mls_group_with_lock_async(async |mls| {
                Ok::<_, GroupError>(
                    xmtp_mls_validation::commit::extract_group_membership_with_failures(
                        mls.extensions(),
                    )?,
                )
            })
            .await?;
        assert_eq!(
            membership.get(caro.client.inbox_id()),
            Some(&caro_sequence),
            "the acknowledged request must add Caro"
        );
        assert!(
            failed
                .of(caro.client.inbox_id())
                .is_some_and(|keys| keys.contains(absent.public_slice()))
        );
        assert!(
            failed
                .of(alix.client.inbox_id())
                .is_none_or(std::collections::HashSet::is_empty)
        );
        assert!(has_leaf(&bo_group, &caro_id, caro.client.inbox_id()).await);
    } else {
        assert_eq!(membership(&bo_group).await, before);
    }
    Ok(())
}

/// Later conflicting requests cannot displace an earlier valid membership delta.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn first_valid_membership_delta_keeps_precedence() {
    use crate::groups::intents::{ProposeMemberUpdateIntentData, QueueIntent};
    let alix = member().await;
    let bo = member().await;
    let caro = member().await;
    let dave = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let intent = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.client.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&alix_group)?;
    alix_group.sync_until_intent_resolved(intent.id).await?;
    let cursor = alix_group
        .context
        .db()
        .topic_progress(&StreamTopic::group(alix_group.group_id))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &bo_group.context,
        [(
            xmtp_proto::types::Topic::new_group_message(bo_group.group_id),
            cursor,
        )]
        .into(),
        None,
    )
    .await?;
    let intent = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![
                caro.client.inbox_id().to_string(),
                dave.client.inbox_id().to_string(),
            ],
            vec![],
        ))?)
        .queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(intent.id).await?;
    let pending_memberships = bo_group
        .load_mls_group_with_lock_async(async |mls| {
            Ok::<_, GroupError>(mls.pending_proposals().filter(|queued| {
            matches!(queued.proposal(), openmls::prelude::Proposal::AppDataUpdate(update)
                if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16())
        }).count())
        })
        .await?;
    assert_eq!(
        pending_memberships, 2,
        "both individually valid requests must be pending"
    );
    let intent = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(intent.id).await?;
    let membership = membership(&bo_group).await;
    assert!(membership.get(caro.client.inbox_id()).is_some());
    assert!(membership.get(dave.client.inbox_id()).is_none());
    assert!(
        has_leaf(
            &bo_group,
            caro.client.context.installation_id().as_slice(),
            caro.client.inbox_id()
        )
        .await
    );
    assert!(
        !has_leaf(
            &bo_group,
            dave.client.context.installation_id().as_slice(),
            dave.client.inbox_id()
        )
        .await
    );
}
