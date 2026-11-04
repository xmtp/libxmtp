//! A commit may only add or replace a leaf whose signature key belongs to the
//! inbox its credential names. Otherwise a member that holds one key could
//! place a leaf under another member's inbox and then send messages and
//! proposals as that member.

use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use crate::groups::{
    GroupError,
    app_data::{membership_upkeep::membership_upkeep, stage_app_data_proposals_and_commit},
    mls_ext::commit_without_group_info,
    mls_sync::{
        generate_prepared_commit, update_group_membership::build_group_membership_app_data_payload,
    },
};
use openmls::{
    messages::proposals::AppDataUpdateOperation,
    prelude::{
        CredentialWithKey, LeafNodeParameters, MlsMessageOut, NewSignerBundle, tls_codec::Serialize,
    },
};
use openmls_traits::OpenMlsProvider;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::{
    TransactionOutcome::Continue,
    XmtpMlsStorageProvider as _, XmtpOpenMlsProviderRef,
    identity_update::QueryIdentityUpdates,
    incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
};
use xmtp_id::associations::{
    InstallationKeyContext, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_wallet_signature},
    unverified::UnverifiedSignature,
};
use xmtp_mls_core::{context::CoreContext, identity::create_credential, state_tx::state_write};
use xmtp_mls_group_state::app_data::component_id::ComponentId;
use xmtp_mls_identity::identity_lifecycle::XmtpKeyPackage;
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

mod leaf_additions;
mod leaf_updates;
mod membership_deltas;

struct Member {
    client: EngineTestPeer,
    wallet: LocalWallet,
}

async fn member() -> Member {
    let client = EngineTestPeer::new_with(|o| o.triggers = true).await;
    let wallet = client.owner.clone();
    Member { client, wallet }
}

/// Alix creates a group with Bo, and Bo joins it.
async fn group(alix: &EngineTestPeer, bo: &EngineTestPeer) -> (EngineTestGroup, EngineTestGroup) {
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
async fn membership(group: &EngineTestGroup) -> GroupMembership {
    group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions()).unwrap())
        })
        .await
        .unwrap()
}

/// Whether the group's tree holds a leaf with `key` whose credential names `inbox_id`.
async fn has_leaf(group: &EngineTestGroup, key: &[u8], inbox_id: &str) -> bool {
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
    receiver: &EngineTestPeer,
    group: &EngineTestGroup,
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
    alix: &EngineTestPeer,
    group: &EngineTestGroup,
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

/// Check the receiver's exact rule error, then its durable network rejection.
async fn assert_rule_rejection(
    group: &EngineTestGroup,
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
