//! A commit may not leave an installation key unaccounted that was
//! accounted for before it: every key an inbox's referenced identity state
//! associates needs a leaf under that inbox or a listing in its failed
//! installations. Otherwise every later joiner rejects the Welcome.

use super::test_leaf_key_binding::{
    Member, assert_rule_rejection, associate, group, member, membership, publish_raw,
};
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeerOps};
use crate::groups::{
    GroupError,
    app_data::stage_app_data_proposals_and_commit,
    intents::QueueIntent,
    mls_ext::commit_without_group_info,
    mls_sync::generate_prepared_commit,
    validated_commit::{CommitValidationError, ValidatedCommit},
};
use openmls::{
    group::MlsGroup as OpenMlsGroup,
    messages::proposals::AppDataUpdateOperation,
    prelude::{CredentialWithKey, MlsMessageOut, NewSignerBundle, tls_codec::Serialize},
};
use openmls_traits::OpenMlsProvider;
use prost::Message as _;
use tls_codec::VLBytes;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_db::{
    ConnectionExt as _, TransactionOutcome::Continue, XmtpOpenMlsProviderRef,
    identity_update::QueryIdentityUpdates, incoming_envelope::QueryIncomingEnvelope,
    incoming_envelope::StreamTopic,
};
use xmtp_mls_core::{context::CoreContext, identity::create_credential, state_tx::state_write};
use xmtp_mls_group_state::{
    app_data::{
        component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
        typed::Component,
    },
    inbox_id::InboxId,
};
use xmtp_mls_identity::identity_updates::{IdentityDependencyError, IdentityRequirement};
use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
use xmtp_mls_validation::commit::CommitRuleError;
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMembershipEntry,
    group_membership_entry::{V1, Version},
};

/// A `GROUP_MEMBERSHIP` delta that sets each inbox's entry to its
/// sequence id and failed installations.
fn set_entries(entries: &[(&str, u64, Vec<Vec<u8>>)]) -> AppDataUpdateOperation {
    let delta = entries.iter().fold(
        TlsMapDelta::<InboxId, VLBytes>::new(),
        |delta, (inbox, sequence_id, failed_installations)| {
            let entry = GroupMembershipEntry {
                version: Some(Version::V1(V1 {
                    sequence_id: *sequence_id,
                    failed_installations: failed_installations.clone(),
                })),
            };
            delta.update(
                InboxId::from_hex(inbox).unwrap(),
                VLBytes::new(entry.encode_to_vec()),
            )
        },
    );
    AppDataUpdateOperation::Update(
        GroupMembershipComponent::encode_mutation(&delta)
            .unwrap()
            .into(),
    )
}

fn unaccounted(inbox: &str) -> impl Fn(&CommitRuleError) -> bool + '_ {
    move |rule| matches!(rule, CommitRuleError::UnaccountedInstallation(i) if i == inbox)
}

/// The receiver's MLS group as it is now, to validate a raw commit against.
fn snapshot(group: &EngineTestGroup) -> OpenMlsGroup {
    OpenMlsGroup::load(group.context.mls_storage(), &group.group_id.to_openmls())
        .unwrap()
        .unwrap()
}

/// Associate a key with `member`'s inbox that has no key package, and
/// return the key and the inbox's latest sequence id.
async fn associate_leafless(member: &Member) -> (Vec<u8>, u64) {
    let key = XmtpInstallationCredential::new();
    associate(member, &key).await;
    let client = &member.client;
    client.inbox_state(true).await.unwrap();
    let sequence_id = client
        .context
        .db()
        .get_latest_sequence_id_for_inbox(client.inbox_id())
        .unwrap() as u64;
    (key.public_slice().to_vec(), sequence_id)
}

/// Bo raises Bo's entry to a state that associates a new key, with no Add
/// for it and no failure listed. Run with Bo or Alix as the committer.
async fn assert_omitted_new_installation_rejected(bo_commits: bool) {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let (_, bo_sequence) = associate_leafless(&bo).await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let (sender, sender_group, receiver_group) = if bo_commits {
        (&bo.client, &bo_group, &alix_group)
    } else {
        (&alix.client, &alix_group, &bo_group)
    };
    let before = membership(receiver_group).await;
    let receiver = snapshot(receiver_group);
    let signer = sender.context.identity().installation_keys.clone();
    let raw = publish_raw!(sender, sender_group, |mls, provider| {
        let (mut messages, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            &xmtp_configuration::LimitsConfiguration::default(),
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                set_entries(&[(&bo_inbox, bo_sequence, vec![])]),
            )],
        )
        .unwrap();
        messages.push(bundle.into_messages().0);
        messages
    });

    assert_rule_rejection(receiver_group, &receiver, raw, unaccounted(&bo_inbox)).await;
    assert_eq!(membership(receiver_group).await, before);
}

// verifies: GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_rejects_omitted_new_installation() {
    assert_omitted_new_installation_rejected(true).await;
}

// verifies: GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_rejects_omitted_new_installation_of_another_inbox() {
    assert_omitted_new_installation_rejected(false).await;
}

/// Alix's refresh lists Bo's leafless key as failed. Alix and Bo, and the
/// key and Bo's sequence id.
async fn group_with_listed_failure() -> (
    Member,
    Member,
    EngineTestGroup,
    EngineTestGroup,
    Vec<u8>,
    u64,
) {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let (absent, bo_sequence) = associate_leafless(&bo).await;
    alix_group.update_installations().await.unwrap();
    bo_group.sync().await.unwrap();
    let listed = membership(&bo_group).await;
    assert_eq!(listed.get(bo.client.inbox_id()), Some(&bo_sequence));
    assert!(listed.failed_installations.contains(&absent));
    (alix, bo, alix_group, bo_group, absent, bo_sequence)
}

/// Rewrite Bo's entry at the same sequence id with its failure removed.
async fn publish_dropped_failure(
    alix: &Member,
    alix_group: &EngineTestGroup,
    bo_inbox: &str,
    bo_sequence: u64,
) -> (
    Option<Vec<u8>>,
    openmls::prelude::LeafNodeIndex,
    xmtp_proto::types::Cursor,
) {
    let alix = &alix.client;
    let signer = alix.context.identity().installation_keys.clone();
    publish_raw!(alix, alix_group, |mls, provider| {
        let (mut messages, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            &xmtp_configuration::LimitsConfiguration::default(),
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                set_entries(&[(bo_inbox, bo_sequence, vec![])]),
            )],
        )
        .unwrap();
        messages.push(bundle.into_messages().0);
        messages
    })
}

/// A commit that keeps Bo's entry but drops its listed failure of a key
/// that is still associated and leafless.
// verifies: GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_rejects_dropped_failure() {
    let (alix, bo, alix_group, bo_group, _, bo_sequence) = group_with_listed_failure().await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let before = membership(&bo_group).await;
    let receiver = snapshot(&bo_group);
    let raw = publish_dropped_failure(&alix, &alix_group, &bo_inbox, bo_sequence).await;

    assert_rule_rejection(&bo_group, &receiver, raw, unaccounted(&bo_inbox)).await;
    assert_eq!(membership(&bo_group).await, before);
}

/// A super admin's commit that raises the floor above Bo's version and
/// drops Bo's listed failure is rejected, not paused on: the accounting rule
/// runs before the floor pause.
// verifies: GMOD-022, GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn floor_raise_does_not_pause_on_an_unaccounting_commit() {
    let (alix, bo, alix_group, bo_group, _, bo_sequence) = group_with_listed_failure().await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let receiver = snapshot(&bo_group);
    let alix_client = &alix.client;
    let signer = alix_client.context.identity().installation_keys.clone();
    let raw = publish_raw!(alix_client, alix_group, |mls, provider| {
        let (mut messages, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            &xmtp_configuration::LimitsConfiguration::default(),
            vec![
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    set_entries(&[(&bo_inbox, bo_sequence, vec![])]),
                ),
                (
                    ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                    AppDataUpdateOperation::Update(b"99.0.0".to_vec().into()),
                ),
            ],
        )
        .unwrap();
        messages.push(bundle.into_messages().0);
        messages
    });

    assert_rule_rejection(&bo_group, &receiver, raw, unaccounted(&bo_inbox)).await;
}

/// When the identity state the rule reads is not cached, the receiver holds
/// the commit instead of rejecting it.
// verifies: GMOD-057, PROC-012
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_holds_commit_when_accounting_state_is_missing() {
    use diesel::{
        RunQueryDsl,
        sql_types::{BigInt, Text},
    };
    let (alix, bo, alix_group, bo_group, _, bo_sequence) = group_with_listed_failure().await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let receiver = snapshot(&bo_group);
    let raw = publish_dropped_failure(&alix, &alix_group, &bo_inbox, bo_sequence).await;
    bo_group.context.db().raw_query(|conn| {
        diesel::sql_query("DELETE FROM association_state WHERE inbox_id = ? AND sequence_id = ?")
            .bind::<Text, _>(&bo_inbox)
            .bind::<BigInt, _>(bo_sequence as i64)
            .execute(conn)
    })?;

    let staged = xmtp_db::db_deserialize(&raw.0?)?;
    let error = ValidatedCommit::from_staged_commit_local(
        &bo_group.context,
        &bo_group.context.db(),
        &staged,
        raw.1,
        &receiver,
        u64::MAX,
    )
    .expect_err("the commit must not be accepted without the state");
    let CommitValidationError::IdentityDependency(IdentityDependencyError::Need(needed)) = &error
    else {
        panic!("expected a hold, got {error:?}");
    };
    assert!(needed.contains(&IdentityRequirement {
        inbox_id: bo_inbox,
        sequence_id: bo_sequence,
    }));
    assert!(!error.is_safe_rejection());
}

/// Alix's refresh lists a second key of Alix's inbox as failed. Alix's
/// update path then rotates Alix's leaf to that key, leaving Alix's
/// current key leafless and not listed.
// verifies: GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn update_path_rotation_cannot_unaccount_old_key() {
    let alix = member().await;
    let bo = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let second = XmtpInstallationCredential::new();
    associate(&alix, &second).await;
    let second_id = second.public_slice().to_vec();
    alix_group.update_installations().await?;
    bo_group.sync().await?;
    assert!(
        membership(&bo_group)
            .await
            .failed_installations
            .contains(&second_id)
    );
    let receiver = snapshot(&bo_group);
    let alix = &alix.client;
    let alix_inbox = alix.inbox_id().to_string();
    let signer = alix.context.identity().installation_keys.clone();
    let credential_with_key = CredentialWithKey {
        credential: create_credential(&alix_inbox)?,
        signature_key: second_id.clone().into(),
    };
    let raw = publish_raw!(alix, alix_group, |mls, provider| {
        let bundle = commit_without_group_info(mls, provider.storage(), |builder| builder)
            .unwrap()
            .build_with_new_signer(
                provider.rand(),
                provider.crypto(),
                &signer,
                NewSignerBundle {
                    signer: &second,
                    credential_with_key,
                },
                |_| true,
            )
            .unwrap()
            .stage_commit(provider)
            .unwrap();
        vec![bundle.into_messages().0]
    });

    assert_rule_rejection(&bo_group, &receiver, raw, unaccounted(&alix_inbox)).await;
}

/// Bo publishes the delta of `receiver_rejects_omitted_new_installation` as
/// a standalone proposal, which every member accepts. Alix's next commit of
/// pending proposals must omit it: otherwise every such commit fails.
// verifies: GMOD-047, GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn pending_unaccounting_delta_is_omitted() {
    let alix = member().await;
    let bo = member().await;
    let caro = member().await;
    let alix_group = alix
        .client
        .create_group_with_members(&[bo.client.inbox_id(), caro.client.inbox_id()], None, None)
        .await?;
    let bo_group = bo.client.wait_for_welcomes().await?.first()?.clone();
    let caro_group = caro.client.wait_for_welcomes().await?.first()?.clone();
    bo_group.sync().await?;
    let (_, bo_sequence) = associate_leafless(&bo).await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let proposal = state_write(bo.client.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &bo.client.context.identity().installation_keys,
                    ComponentId::GROUP_MEMBERSHIP.as_u16(),
                    set_entries(&[(&bo_inbox, bo_sequence, vec![])]),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
    bo.client
        .context
        .api()
        .send_group_messages(messages)
        .await?;
    // Receive without the installation refresh `sync` and sends run, which
    // would account for the key itself and supersede the delta.
    for group in [&alix_group, &caro_group] {
        group.receive().await?;
        assert_eq!(
            group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            1,
            "members accept the standalone delta"
        );
    }
    let before = membership(&alix_group).await;

    let intent = QueueIntent::commit_pending_proposals().queue(&alix_group)?;
    alix_group.sync_until_intent_resolved(intent.id).await?;
    for group in [&bo_group, &caro_group] {
        group.receive().await?;
    }
    let epoch = alix_group.epoch().await?;
    assert!(epoch > 1, "the pending proposals were committed");
    for group in [&bo_group, &caro_group] {
        assert_eq!(group.epoch().await?, epoch);
        assert_eq!(membership(group).await, before, "the delta was omitted");
    }
    assert!(
        caro.client
            .context
            .db()
            .read_last_rejection(&StreamTopic::group(caro_group.group_id))?
            .is_none(),
        "no member rejects the commit"
    );
    alix_group.send_message(b"hi", Default::default()).await?;
    caro_group.update_group_name("after".into()).await?;
}

/// After a receiver rejects a commit that leaves Bo's new key unaccounted,
/// Bo's entry still accounts for every key, so a joiner accepts the
/// Welcome.
// verifies: GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn joiner_accepts_welcome_after_incomplete_bump_rejected() {
    let alix = member().await;
    let bo = member().await;
    let caro = member().await;
    let (alix_group, bo_group) = group(&alix.client, &bo.client).await;
    let (_, bo_sequence) = associate_leafless(&bo).await;
    let bo_inbox = bo.client.inbox_id().to_string();
    let bo_client = &bo.client;
    let signer = bo_client.context.identity().installation_keys.clone();
    publish_raw!(bo_client, bo_group, |mls, provider| {
        let (mut messages, bundle) = stage_app_data_proposals_and_commit(
            mls,
            provider,
            &signer,
            &[],
            &xmtp_configuration::LimitsConfiguration::default(),
            vec![(
                ComponentId::GROUP_MEMBERSHIP,
                set_entries(&[(&bo_inbox, bo_sequence, vec![])]),
            )],
        )
        .unwrap();
        messages.push(bundle.into_messages().0);
        messages
    });
    alix_group.sync().await?;

    alix_group.add_members(&[caro.client.inbox_id()]).await?;
    let caro_group = caro.client.wait_for_welcomes().await?.first()?.clone();
    caro_group.sync().await?;
    assert_eq!(caro_group.epoch().await?, alix_group.epoch().await?);
}
