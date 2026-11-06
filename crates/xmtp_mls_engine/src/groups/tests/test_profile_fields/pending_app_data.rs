//! Profile field tests: pending app data.

use super::*;

/// Publish `operation` on `id` as a signed standalone proposal with no
/// commit, the way a modified client can.
async fn propose_raw<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
    operation: AppDataUpdateOperation,
) -> Result<(), GroupError> {
    let signer = &group.context.identity().installation_keys;
    let message = state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    signer,
                    id.as_u16(),
                    operation.clone(),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = group.prepare_group_messages(vec![(message.as_slice(), false)])?;
    group.context.api().send_group_messages(messages).await?;
    Ok(())
}

/// Pending proposals a member holds that no commit can carry together.
enum Uncommittable {
    /// Bo inserts its display name again.
    CommittedName,
    /// Bo inserts its absent display name, then a second one for the same key.
    PendingName,
    /// Bo proposes one insert of its absent display name twice. Both
    /// proposals have the same reference.
    Replay,
    /// Alix holds two copies of one insert, as a build before receive-time
    /// deduplication stored a replay.
    StoredTwice,
    /// Alix inserts an existing admin. Receivers refuse it, but Alix holds
    /// her own copy, which blocked her own commits.
    CommittedAdmin,
    /// Alix updates the description, then removes it: a list may not
    /// update and remove one component.
    UpdateThenRemove,
    /// Alix removes the absent image URL: a list may only remove a
    /// component with state.
    RemoveAbsent,
    /// Alix and Bo are super admins, and Alix removes Bo, then herself:
    /// together they leave none.
    NoSuperAdminLeft,
    /// Alix deletes the membership's registry entry. It applies, but the
    /// registry it leaves has no policy for a membership change.
    RegistryWithoutMembership,
}

/// Receivers judge a standalone `AppDataUpdate` alone against the committed
/// dictionary, so they accept proposals that no commit can carry together.
/// Every commit of pending proposals once carried them all and failed, so
/// no member could send or change the group again. A commit omits each
/// proposal it cannot carry instead, keeping the ones before it.
// verifies: GMOD-047, GMOD-045
#[rstest::rstest]
#[case::insert_of_committed_name(Uncommittable::CommittedName)]
#[case::second_pending_insert(Uncommittable::PendingName)]
#[case::replayed_insert(Uncommittable::Replay)]
#[case::insert_stored_twice(Uncommittable::StoredTwice)]
#[case::insert_of_committed_admin(Uncommittable::CommittedAdmin)]
#[case::update_then_remove(Uncommittable::UpdateThenRemove)]
#[case::remove_of_absent_component(Uncommittable::RemoveAbsent)]
#[case::no_super_admin_left(Uncommittable::NoSuperAdminLeft)]
#[case::registry_without_membership(Uncommittable::RegistryWithoutMembership)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_uncommittable_pending_app_data_does_not_block_the_group(
    #[case] uncommittable: Uncommittable,
    #[values(false, true)] field_write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let update = |payload: Vec<u8>| AppDataUpdateOperation::Update(payload.into());
    let bo_name = || update(insert_name(bo.inbox_id(), b"Bo"));
    let remove_super_admin = |id: &str| {
        update(
            xmtp_mls_tls_collections::tls_set::TlsSetDelta::new()
                .remove(inbox(id))
                .tls_serialize_detached()
                .unwrap(),
        )
    };
    let names = ComponentId::USER_DISPLAY_NAME;
    // A member that holds the proposals, and how many.
    let (holder, proposed) = match uncommittable {
        Uncommittable::CommittedName => {
            write(&bo_group, names, insert_name(bo.inbox_id(), b"Bo")).await?;
            propose_raw(&bo_group, names, bo_name()).await?;
            (&group, 1)
        }
        Uncommittable::PendingName => {
            propose_raw(&bo_group, names, bo_name()).await?;
            let other = update(insert_name(bo.inbox_id(), b"Mallory"));
            propose_raw(&bo_group, names, other).await?;
            (&group, 2)
        }
        Uncommittable::Replay => {
            for _ in 0..2 {
                propose_raw(&bo_group, names, bo_name()).await?;
            }
            // Receivers store a replay once.
            (&group, 1)
        }
        Uncommittable::StoredTwice => {
            propose_raw(&bo_group, names, bo_name()).await?;
            group.sync().await?;
            state_write(group.context.mls_storage(), |tx| {
                tx.with_group(group.group_id, |mls, storage| {
                    let queued = mls.pending_proposals().next().cloned().unwrap();
                    mls.store_pending_proposal(storage, queued)?;
                    Ok::<_, GroupError>(Continue(()))
                })
            })?;
            (&group, 2)
        }
        Uncommittable::CommittedAdmin => {
            group
                .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
                .await?;
            let payload = xmtp_mls_tls_collections::tls_set::TlsSetDelta::new()
                .insert(inbox(bo.inbox_id()))
                .tls_serialize_detached()?;
            propose_raw(&group, ComponentId::ADMIN_LIST, update(payload)).await?;
            (&group, 1)
        }
        Uncommittable::UpdateThenRemove => {
            let description = ComponentId::GROUP_DESCRIPTION;
            propose_raw(&group, description, update(b"kept".to_vec())).await?;
            propose_raw(&group, description, AppDataUpdateOperation::Remove).await?;
            (&bo_group, 2)
        }
        Uncommittable::RemoveAbsent => {
            assert_eq!(value(&group, ComponentId::GROUP_IMAGE_URL)?, None);
            let remove = AppDataUpdateOperation::Remove;
            propose_raw(&group, ComponentId::GROUP_IMAGE_URL, remove).await?;
            (&bo_group, 1)
        }
        Uncommittable::NoSuperAdminLeft => {
            group
                .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
                .await?;
            let super_admins = ComponentId::SUPER_ADMIN_LIST;
            propose_raw(&group, super_admins, remove_super_admin(bo.inbox_id())).await?;
            propose_raw(&group, super_admins, remove_super_admin(alix.inbox_id())).await?;
            (&bo_group, 2)
        }
        Uncommittable::RegistryWithoutMembership => {
            let payload = TlsMapDelta::<ComponentId, VLBytes>::new()
                .delete(ComponentId::GROUP_MEMBERSHIP)
                .tls_serialize_detached()?;
            propose_raw(&group, ComponentId::COMPONENT_REGISTRY, update(payload)).await?;
            (&group, 1)
        }
    };
    holder.sync().await?;
    assert_eq!(
        holder.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        proposed,
        "the member holds each proposal"
    );

    // A field write reads the pending state before it queues its commit.
    if field_write {
        let rename = ComponentMutation::Replace(FieldValue::String("after".into()));
        group
            .update_metadata_field(&MetadataFieldRef::GROUP_NAME, &rename)
            .await?;
    }
    for member in [&group, &bo_group] {
        member.sync().await?;
        member.send_message(b"after", Default::default()).await?;
    }
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
        if field_write {
            assert_eq!(member.group_name()?, "after");
        }
        match uncommittable {
            Uncommittable::CommittedAdmin => {
                assert_eq!(admins(member)?, vec![inbox(bo.inbox_id())]);
            }
            Uncommittable::UpdateThenRemove => assert_eq!(
                value(member, ComponentId::GROUP_DESCRIPTION)?,
                Some(b"kept".to_vec())
            ),
            Uncommittable::RemoveAbsent => {
                assert_eq!(value(member, ComponentId::GROUP_IMAGE_URL)?, None);
            }
            Uncommittable::NoSuperAdminLeft => assert_eq!(
                inboxes(member, ComponentId::SUPER_ADMIN_LIST)?,
                vec![inbox(alix.inbox_id())]
            ),
            Uncommittable::RegistryWithoutMembership => {
                assert!(member.with_group_snapshot(|mls| {
                    Ok(load_component_registry(mls)?
                        .get(&ComponentId::GROUP_MEMBERSHIP)
                        .unwrap()
                        .is_some())
                })?)
            }
            _ => assert_eq!(
                display_names(member)?,
                BTreeMap::from([(inbox(bo.inbox_id()), b"Bo".to_vec())])
            ),
        }
    }
    Ok(())
}

/// A build before receive-time deduplication could store one membership
/// delta twice. The membership selection omits the second copy, and
/// removing it by reference deleted the proposal both copies share, so
/// reloading the group failed and no member could send again. The
/// selection rebuilds the store instead.
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_delta_stored_twice_does_not_block_the_group()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    propose_removal(&group, carol.inbox_id()).await?;
    state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let delta = mls
                .pending_proposals()
                .find(|queued| {
                    matches!(queued.proposal(), Proposal::AppDataUpdate(update)
                        if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16())
                })
                .cloned()
                .unwrap();
            mls.store_pending_proposal(storage, delta)?;
            Ok::<_, GroupError>(Continue(()))
        })
    })?;

    group.send_message(b"after", Default::default()).await?;
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
    }
    Ok(())
}

/// Receivers check only that the leaf in a standalone Update proposal
/// names its proposer's inbox. A commit checks that the leaf keeps the key
/// of the leaf it replaces and that the key is an installation of that
/// inbox. Every commit of pending proposals carried one with a new key and
/// was refused, so no member could send again. No client proposes a leaf
/// Update, so a commit never carries one.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_leaf_update_does_not_block_the_group()
-> Result<(), Box<dyn std::error::Error>> {
    use openmls::prelude::{CredentialWithKey, LeafNodeParameters, NewSignerBundle};
    use xmtp_cryptography::XmtpInstallationCredential;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let identity = bo_group.context.identity();
    let unregistered = XmtpInstallationCredential::new();
    let message = state_write(bo_group.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let new_signer = NewSignerBundle {
                signer: &unregistered,
                credential_with_key: CredentialWithKey {
                    credential: identity.credential(),
                    signature_key: unregistered.public_slice().into(),
                },
            };
            let (message, _) = mls
                .propose_self_update_with_new_signer(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    &identity.installation_keys,
                    new_signer,
                    LeafNodeParameters::default(),
                )
                .unwrap();
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(message.as_slice(), false)])?;
    bo_group.context.api().send_group_messages(messages).await?;
    group.sync().await?;
    assert_eq!(
        group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        1
    );

    group.send_message(b"after", Default::default()).await?;
    bo_group.sync().await?;
    bo_group.send_message(b"after", Default::default()).await?;
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    Ok(())
}

/// A receiver judges a standalone Delete of a non-member's display name
/// against the committed membership, which allows it. A pending proposal
/// that adds the owner back makes the owner a member after the commit,
/// where the Delete is refused. The commit judges each proposal against
/// the membership the kept proposals leave, so it omits the Delete.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_readd_omits_a_delete_it_forbids() -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), carol.inbox_id(), dave.inbox_id()],
            None,
            None,
        )
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let carol_group = carol.wait_for_welcomes().await?.remove(0);
    let dave_group = dave.wait_for_welcomes().await?.remove(0);
    let names = ComponentId::USER_DISPLAY_NAME;
    write(&carol_group, names, insert_name(carol.inbox_id(), b"Carol")).await?;
    group.sync().await?;
    // A removal without cleanup leaves Carol's name behind.
    propose_removal(&group, carol.inbox_id()).await?;
    publish_unchecked(&group, vec![]).await?;
    for member in [&bo_group, &dave_group] {
        member.sync().await?;
    }

    propose_members(&bo_group, vec![carol.inbox_id().to_string()], vec![]).await?;
    let delete = TlsMapDelta::<InboxId, VLBytes>::new()
        .delete(inbox(carol.inbox_id()))
        .tls_serialize_detached()?;
    let delete = AppDataUpdateOperation::Update(delete.into());
    propose_raw(&dave_group, names, delete).await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        3,
        "Bo holds the Add, its membership delta, and Dave's Delete"
    );

    bo_group.send_message(b"after", Default::default()).await?;
    dave_group.sync().await?;
    for member in [&bo_group, &dave_group] {
        assert_eq!(member.members().await?.len(), 4);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(carol.inbox_id()), b"Carol".to_vec())])
        );
    }
    Ok(())
}

/// A standalone membership delta with no Add or Remove of its inbox.
enum Orphan {
    /// Bo inserts Carol, who is not a member.
    Insert,
    /// Alix deletes Bo, who stays in the tree.
    Delete,
}

/// Receivers check a standalone membership delta's policy and bounds, not
/// that an Add or Remove of its inbox comes with it. A commit that carries
/// one without them is refused (`ProposerNotFound`): the pre-publish check
/// failed every commit of pending proposals, and no member could send
/// again. The membership selection omits the delta instead.
// verifies: GMOD-038, GMOD-047
#[rstest::rstest]
#[case::insert(Orphan::Insert)]
#[case::delete(Orphan::Delete)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_orphan_membership_delta_does_not_block_the_group(
    #[case] orphan: Orphan,
) -> Result<(), Box<dyn std::error::Error>> {
    use prost::Message as _;
    use xmtp_mls_group_state::app_data::{
        components::tls_map_components::GroupMembershipComponent, typed::Component,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let carol = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let (proposer, delta) = match orphan {
        Orphan::Insert => {
            let sequence_id = carol
                .context
                .db()
                .get_latest_sequence_id_for_inbox(carol.inbox_id())?
                as u64;
            let entry = GroupMembershipEntry {
                version: Some(Version::V1(V1 {
                    sequence_id,
                    failed_installations: vec![],
                })),
            };
            let delta = TlsMapDelta::<InboxId, VLBytes>::new()
                .insert(inbox(carol.inbox_id()), VLBytes::new(entry.encode_to_vec()));
            (&bo_group, delta)
        }
        Orphan::Delete => (
            &group,
            TlsMapDelta::<InboxId, VLBytes>::new().delete(inbox(bo.inbox_id())),
        ),
    };
    let payload = <GroupMembershipComponent as Component>::encode_mutation(&delta).unwrap();
    let update = AppDataUpdateOperation::Update(payload.into());
    propose_raw(proposer, ComponentId::GROUP_MEMBERSHIP, update).await?;

    for member in [&group, &bo_group] {
        member.sync().await?;
        member.send_message(b"after", Default::default()).await?;
    }
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    Ok(())
}

/// Omitting a proposal no commit can carry does not judge the rest before
/// publication: receivers judge them on read-back, and a rejection fails
/// only the intent that carried them. The omission once also ran the
/// membership selection's pre-publish check, whose refusal left the intent
/// to retry forever, so every later intent of that member waited behind it.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn test_omission_does_not_stall_a_conflicting_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    group.update_group_description("before".into()).await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let description = ComponentId::GROUP_DESCRIPTION;
    propose_raw(&group, description, AppDataUpdateOperation::Remove).await?;
    // Receivers accept both; a commit omits the second.
    for name in [b"Alix".as_slice(), b"Mallory"] {
        let insert = insert_name(alix.inbox_id(), name);
        let insert = AppDataUpdateOperation::Update(insert.into());
        propose_raw(&group, ComponentId::USER_DISPLAY_NAME, insert).await?;
    }
    bo_group.sync().await?;

    // A list may not remove and update one component, so receivers refuse
    // the commit that carries the Remove and Bo's own update.
    assert!(
        bo_group
            .update_group_description("after".into())
            .await
            .is_err()
    );
    bo_group.send_message(b"after", Default::default()).await?;
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(value(member, description)?, None);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(alix.inbox_id()), b"Alix".to_vec())])
        );
    }
    Ok(())
}
