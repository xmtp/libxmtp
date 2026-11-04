//! Leaf key binding tests: membership deltas.

use super::*;

// verifies: GMOD-049
#[xmtp_common::test(unwrap_try = true)]
async fn receiver_rejects_misattributed_failed_installation() {
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::{
        app_data::{components::tls_map_components::GroupMembershipComponent, typed::Component},
        inbox_id::InboxId,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
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
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;

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
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::intents::{ProposeMemberUpdateIntentData, QueueIntent};
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_core::mls_store::MlsStore;
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
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
