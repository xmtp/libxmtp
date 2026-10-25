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
        let payloads = state_write(storage, |tx| {
            tx.with_group($group.group_id, |mls_group, storage| {
                let (messages, _staged_commit, _epoch) =
                    generate_prepared_commit(storage, mls_group, |$mls, $provider| {
                        let messages: Vec<MlsMessageOut> = $build;
                        Ok::<_, GroupError>(messages)
                    })?;
                let payloads = messages
                    .iter()
                    .map(Serialize::tls_serialize_detached)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, GroupError>(Continue(payloads))
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
        $sender
            .context
            .api()
            .send_group_messages(messages)
            .await
            .unwrap();
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
    let bo_group = bo.wait_for_welcomes().await.unwrap().remove(0);
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

/// Alix registers a new installation on Alix's own inbox, then adds its key
/// package with a credential that names Bo. The key is in the expected
/// additions, but it does not belong to Bo.
// verifies: GMOD-012
#[xmtp_common::test(unwrap_try = true)]
async fn add_leaf_must_belong_to_credential_inbox() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let forged_key = XmtpInstallationCredential::new();
    associate(&alix, &forged_key).await;
    let forged_key_id = forged_key.public_slice().to_vec();

    // Alix's entry must reference the identity state that adds the new key.
    let alix = &alix.client;
    alix.inbox_state(true).await?;
    let alix_sequence = alix
        .context
        .db()
        .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;

    let bo_inbox_id = bo.client.inbox_id().to_string();
    let forged_package = XmtpKeyPackage::builder()
        .inbox_id(bo_inbox_id.clone())
        .credential(create_credential(&bo_inbox_id)?)
        .installation_keys(forged_key)
        .build(&alix.context.mls_provider(), false)?
        .key_package;
    let signer = alix.context.identity().installation_keys.clone();
    let db = alix.context.db();
    publish_raw!(alix, alix_group, |mls, provider| {
        let old = extract_group_membership(mls.extensions()).unwrap();
        let mut new = old.clone();
        new.add(alix.inbox_id().to_string(), alix_sequence);
        let payload = build_group_membership_app_data_payload(&db, mls, &old, &new).unwrap();
        let (add, _) = mls
            .propose_add_member(provider, &signer, &forged_package)
            .unwrap();
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
        let (commit, _welcome, _) = bundle.into_messages();
        let mut messages = vec![add];
        messages.extend(proposals);
        messages.push(commit);
        messages
    });

    assert_rejected(&bo.client, &bo_group, &forged_key_id, &bo_inbox_id).await;
}

/// Alix commits an update path whose leaf keeps Alix's key but names Bo's
/// inbox. OpenMLS accepts any credential in the update path leaf.
// verifies: GMOD-012
#[xmtp_common::test(unwrap_try = true)]
async fn update_path_leaf_must_not_claim_another_inbox() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let bo_inbox_id = bo.client.inbox_id().to_string();
    publish_path_credential(&alix.client, &alix_group, &bo_inbox_id, None).await;

    let alix_key = alix.client.installation_public_key().to_vec();
    assert_rejected(&bo.client, &bo_group, &alix_key, &bo_inbox_id).await;
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

/// Bo registers a new installation on Bo's inbox and publishes its key
/// package with a credential that names Alix. A membership refresh must not
/// add that package, which every receiver would reject. It records the
/// installation as failed instead, and the refresh still succeeds.
// verifies: JOIN-012
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
