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

/// A peer whose catalogue has one application map, typed by its registry
/// entry, that any member may write.
async fn peer_with_application_map(id: ComponentId) -> EngineTestPeer {
    EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = vec![definition(
                id.as_u16(),
                ComponentType::TlsMapBytesBytes,
                MetadataBasePolicy::Allow,
                true,
                false,
            )]
        })
    })
    .await
}

/// An `Update` payload of `delta` for a byte-keyed map.
fn map_update(delta: TlsMapDelta<VLBytes, VLBytes>) -> AppDataUpdateOperation {
    AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
}

/// A member stores a standalone Update of a key that another pending
/// proposal inserts, in an application map typed by its registry entry: on
/// receipt the Update is not judged against the committed map, where the
/// key is absent. The commit that carries the insert first applies both.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_of_a_key_a_pending_insert_creates_is_stored()
-> Result<(), Box<dyn std::error::Error>> {
    let id = ComponentId::new(0xC1A0);
    let alix = peer_with_application_map(id).await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let key = || VLBytes::new(b"k".to_vec());
    let insert = TlsMapDelta::new().insert(key(), VLBytes::new(b"a".to_vec()));
    propose_raw(&bo_group, id, map_update(insert)).await?;
    let update = TlsMapDelta::new().update(key(), VLBytes::new(b"b".to_vec()));
    propose_raw(&bo_group, id, map_update(update)).await?;
    group.sync().await?;
    assert_eq!(
        group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        2,
        "the member refused the update of a key only a pending insert creates"
    );

    group.send_message(b"after", Default::default()).await?;
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        let map = TlsMap::<VLBytes, VLBytes>::tls_deserialize_exact(value(member, id)?.unwrap())?;
        assert_eq!(map.get(&key()), Some(&VLBytes::new(b"b".to_vec())));
    }
    Ok(())
}

/// Staging never resolves a component's type for a `Remove`. So a commit
/// that removes an application component whose registry entry an earlier
/// commit deleted fails validation, not staging: the write is denied by
/// default. Every member rejects it with `InsufficientPermissions` and
/// evicts the pending proposals it carried.
// verifies: PERM-012
#[xmtp_common::test(unwrap_try = true)]
async fn test_remove_of_an_unregistered_component_fails_validation()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::{
        app_data::accumulate_app_data_updates, mls_ext::commit_without_group_info,
    };
    use openmls::messages::proposals::AppDataUpdateProposal;
    use openmls_traits::OpenMlsProvider as _;

    let id = ComponentId::new(0xC1A1);
    let alix = peer_with_application_map(id).await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let entry = TlsMapDelta::<VLBytes, VLBytes>::new()
        .insert(VLBytes::new(b"k".to_vec()), VLBytes::new(b"v".to_vec()));
    write(&group, id, entry.tls_serialize_detached()?).await?;
    // Bo, a super admin whose catalogue lacks the map, deletes its registry
    // entry. Its commits do not register the map again.
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    bo_group.sync().await?;
    let unregister = TlsMapDelta::<ComponentId, VLBytes>::new().delete(id);
    write(
        &bo_group,
        ComponentId::COMPONENT_REGISTRY,
        unregister.tls_serialize_detached()?,
    )
    .await?;
    let pending = |member: &MlsGroup<_>| {
        member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))
    };
    let name = AppDataUpdateOperation::Update(insert_name(bo.inbox_id(), b"Bo").into());
    propose_raw(&bo_group, ComponentId::USER_DISPLAY_NAME, name).await?;
    for member in [&group, &bo_group, &caro_group] {
        member.sync().await?;
        let registered = member.with_group_snapshot(|mls| {
            Ok(load_component_registry(mls)?.get(&id).unwrap().is_some())
        })?;
        assert!(!registered, "the registry entry is still present");
        assert!(value(member, id)?.is_some());
        assert_eq!(pending(member)?, 1, "a member did not store the name");
    }
    let epoch = group.epoch().await?;

    // Caro commits the name by reference with an inline Remove of the map.
    // Its dictionary is built here, not by its own staging.
    let signer = &caro.context.identity().installation_keys;
    let commit = state_write(caro.context.mls_storage(), |tx| {
        tx.with_group(caro_group.group_id, |mls, storage| {
            let (bundle, _, _) = generate_prepared_commit(storage, mls, |group, provider| {
                let pending: Vec<_> = group
                    .pending_proposals()
                    .filter_map(|queued| match queued.proposal() {
                        Proposal::AppDataUpdate(p) => {
                            Some((p.component_id(), p.operation().clone()))
                        }
                        _ => None,
                    })
                    .collect();
                let updates =
                    accumulate_app_data_updates(group, pending.iter().map(|(id, op)| (*id, op)))?;
                let mut updater = group.app_data_dictionary_updater();
                for (component, state) in updates.into_iter().flatten() {
                    match state {
                        Some(bytes) => {
                            updater.set(ComponentData::from_parts(component, bytes.into()))
                        }
                        None => updater.remove(&component),
                    }
                }
                updater.remove(&id.as_u16());
                let updates = updater.changes();
                let remove = AppDataUpdateProposal::remove(id.as_u16());
                let mut stage = commit_without_group_info(group, provider.storage(), |builder| {
                    builder
                        .consume_proposal_store(true)
                        .add_proposal(Proposal::AppDataUpdate(Box::new(remove)))
                })
                .unwrap();
                stage.with_app_data_dictionary_updates(updates);
                let bundle = stage
                    .build(provider.rand(), provider.crypto(), signer, |_| true)
                    .unwrap()
                    .stage_commit(provider)
                    .unwrap();
                Ok::<_, GroupError>(bundle)
            })?;
            let (commit, _, _) = bundle.into_messages();
            Ok::<_, GroupError>(Continue(commit.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = caro_group.prepare_group_messages(vec![(commit.as_slice(), false)])?;
    caro.context.api().send_group_messages(messages).await?;

    for member in [&group, &bo_group] {
        member.sync().await?;
        assert_eq!(member.epoch().await?, epoch, "a member applied the commit");
        assert_eq!(
            last_rejection(member)?.map(|rejection| rejection.code),
            Some("commit_validation_failure".to_string())
        );
        assert_eq!(pending(member)?, 0, "the carried proposal was not evicted");
        assert!(value(member, id)?.is_some());
    }
    Ok(())
}

/// A byte-string key or value.
fn bytes(value: impl Into<Vec<u8>>) -> VLBytes {
    VLBytes::new(value.into())
}

/// The two-byte key of entry `n` of an application map.
fn nth_key(n: u16) -> VLBytes {
    bytes(n.to_be_bytes())
}

/// Write `entries` empty-valued entries to application map `id`.
async fn fill_map<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
    entries: u16,
) -> Result<(), GroupError> {
    let fill = (0..entries).fold(TlsMapDelta::new(), |delta, n| {
        delta.insert(nth_key(n), bytes(vec![]))
    });
    write(group, id, fill.tls_serialize_detached()?).await
}

/// Publish one commit by `author` that carries `operations` on `id` as
/// inline proposals, staged the way a sender that skips its own checks
/// would.
async fn publish_inline<C: XmtpSharedContext>(
    author: &MlsGroup<C>,
    id: ComponentId,
    operations: &[AppDataUpdateOperation],
) -> Result<(), GroupError> {
    use crate::groups::{
        app_data::accumulate_app_data_updates, mls_ext::commit_without_group_info,
    };
    use openmls::messages::proposals::AppDataUpdateProposal;
    use openmls_traits::OpenMlsProvider as _;

    let signer = &author.context.identity().installation_keys;
    let commit = state_write(author.context.mls_storage(), |tx| {
        tx.with_group(author.group_id, |mls, storage| {
            let (bundle, _, _) = generate_prepared_commit(storage, mls, |group, provider| {
                let updates = accumulate_app_data_updates(
                    group,
                    operations.iter().map(|op| (id.as_u16(), op)),
                )?;
                let proposals: Vec<_> = operations
                    .iter()
                    .map(|operation| {
                        let proposal = match operation {
                            AppDataUpdateOperation::Update(payload) => {
                                AppDataUpdateProposal::update(
                                    id.as_u16(),
                                    payload.as_slice().to_vec(),
                                )
                            }
                            AppDataUpdateOperation::Remove => {
                                AppDataUpdateProposal::remove(id.as_u16())
                            }
                        };
                        Proposal::AppDataUpdate(Box::new(proposal))
                    })
                    .collect();
                let mut stage = commit_without_group_info(group, provider.storage(), |builder| {
                    builder.add_proposals(proposals)
                })
                .unwrap();
                stage.with_app_data_dictionary_updates(updates);
                let bundle = stage
                    .build(provider.rand(), provider.crypto(), signer, |_| true)
                    .unwrap()
                    .stage_commit(provider)
                    .unwrap();
                Ok::<_, GroupError>(bundle)
            })?;
            let (commit, _, _) = bundle.into_messages();
            Ok::<_, GroupError>(Continue(commit.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = author.prepare_group_messages(vec![(commit.as_slice(), false)])?;
    author.context.api().send_group_messages(messages).await?;
    Ok(())
}

/// Staging a commit of 5,000 one-mutation Updates to a 2,000-entry
/// application map decodes the map once, not once per proposal, and leaves
/// the map that applying the Updates in order gives.
#[xmtp_common::test(unwrap_try = true)]
async fn test_staging_batch_decodes_once() -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::app_data::accumulate_app_data_updates;
    use xmtp_mls_group_state::app_data::working_set::counters::count_snapshot_decodes;

    const ENTRIES: u16 = 2_000;
    const UPDATES: u16 = 5_000;
    let id = ComponentId::new(0xC1A2);
    let alix = peer_with_application_map(id).await;
    let group = alix.create_group(None, None)?;
    fill_map(&group, id, ENTRIES).await?;
    let mut expected =
        TlsMap::<VLBytes, VLBytes>::tls_deserialize_exact(value(&group, id)?.unwrap())?;
    let operations: Vec<_> = (0..UPDATES)
        .map(|n| {
            let key = nth_key(n % ENTRIES);
            expected.set(key.clone(), bytes(n.to_le_bytes()));
            map_update(TlsMapDelta::new().update(key, bytes(n.to_le_bytes())))
        })
        .collect();

    let (staged, decodes) = group.with_group_snapshot(|mls| {
        Ok(count_snapshot_decodes(|| {
            accumulate_app_data_updates(mls, operations.iter().map(|op| (id.as_u16(), op)))
        }))
    })?;
    // The registry that types the payloads, and the map.
    assert!(decodes <= 2, "{decodes} snapshot decodes");
    let staged = staged?
        .into_iter()
        .flatten()
        .find(|(component, _)| *component == id.as_u16())
        .and_then(|(_, state)| state);
    assert_eq!(staged, Some(expected.tls_serialize_detached()?));
    Ok(())
}

/// A commit builder judges the group's pending proposals with one batch:
/// 300 pending proposals to a 2,000-entry application map, every other one
/// an 8,000-byte insert that the builder's dictionary bound refuses, cost a
/// few snapshot decodes and encodes per builder pass, not one per proposal
/// or per refusal.
#[xmtp_common::test(unwrap_try = true)]
async fn test_keep_decodes_once() -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::app_data::Committable;
    use xmtp_mls_group_state::app_data::working_set::counters::{count_snapshot_decodes, encodes};
    use xmtp_mls_validation::commit::dictionary_total;

    const ENTRIES: u16 = 2_000;
    const PENDING: u16 = 300;
    /// Decodes, and encodes, of the registry, the membership and the map
    /// that one builder pass over pending proposals to one map takes,
    /// however many it judges or refuses.
    const WORK: usize = 16;
    let id = ComponentId::new(0xC1A6);
    let alix = peer_with_application_map(id).await;
    let group = alix.create_group(None, None)?;
    fill_map(&group, id, ENTRIES).await?;
    // Alix's own pending proposals, stored without a commit: one-mutation
    // Updates, each followed by an insert of a new 8,000-byte value.
    let operations: Vec<_> = (0..PENDING)
        .map(|n| {
            map_update(if n % 2 == 0 {
                TlsMapDelta::new().update(nth_key(n), bytes(n.to_le_bytes()))
            } else {
                TlsMapDelta::new().insert(nth_key(ENTRIES + n), bytes(vec![b'x'; 8_000]))
            })
        })
        .collect();
    let signer = &alix.context.identity().installation_keys;
    state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let provider = xmtp_db::XmtpOpenMlsProviderRef::new(storage);
            for operation in &operations {
                mls.propose_app_data_update(&provider, signer, id.as_u16(), operation.clone())
                    .map_err(GroupError::Proposal)?;
            }
            Ok::<_, GroupError>(Continue(()))
        })
    })?;

    let (kept, decodes, encoded) = group.with_group_snapshot(|mls| {
        let committed = dictionary_total(
            mls.extensions()
                .app_data_dictionary()
                .map(|extension| extension.dictionary()),
        );
        let before = encodes();
        // Room for the Updates, not for any of the inserts.
        let (committable, decodes) =
            count_snapshot_decodes(|| Committable::pending(mls, usize::MAX, committed + 1_000));
        let kept: Vec<_> = committable?
            .proposals
            .iter()
            .filter_map(|queued| match queued.proposal() {
                Proposal::AppDataUpdate(proposal) => Some(proposal.operation().clone()),
                _ => None,
            })
            .collect();
        Ok((kept, decodes, encodes() - before))
    })?;
    let updates: Vec<_> = operations.into_iter().step_by(2).collect();
    assert_eq!(kept, updates);
    assert!(
        decodes <= WORK && encoded <= WORK,
        "{decodes} snapshot decodes, {encoded} encodes"
    );
    Ok(())
}

/// A commit builder keeps each pending proposal that receivers accept after
/// the ones kept before it, and the ones it refuses leave no trace. A
/// proposal that fails at its third mutation of five changes nothing, as no
/// failed update does. A proposal that applies but grows the dictionary past
/// the builder's bound is rolled back by the builder, entries and running
/// total both: the later insert of the key it wrote is kept, and the bound
/// still has room for it. The kept proposals leave the map they leave alone.
// verifies: GMOD-047, META-078
#[xmtp_common::test(unwrap_try = true)]
async fn test_keep_rolls_back_failed_proposal() -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::app_data::Committable;
    use xmtp_mls_validation::commit::dictionary_total;

    let id = ComponentId::new(0xC1A3);
    let alix = peer_with_application_map(id).await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let insert = |key: &str, value: Vec<u8>| TlsMapDelta::new().insert(bytes(key), bytes(value));
    write(
        &group,
        id,
        insert("k0", b"v".to_vec()).tls_serialize_detached()?,
    )
    .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    bo_group.sync().await?;
    let deltas = [
        insert("k1", b"v".to_vec()),
        // Fails at its third mutation: `missing` is absent. The failed
        // update undoes its first two inserts itself.
        insert("a", b"v".to_vec())
            .insert(bytes("b"), bytes("v"))
            .update(bytes("missing"), bytes("v"))
            .insert(bytes("c"), bytes("v"))
            .insert(bytes("d"), bytes("v")),
        // Applies only if the failed proposal's inserts were undone.
        insert("a", b"v".to_vec()),
        // Applies, but grows the dictionary past the bound below. Only the
        // builder's rollback undoes it.
        insert("big", vec![b'x'; 8_000]),
        // Applies only if the oversized insert was undone, and fits only if
        // the running total was restored too.
        insert("big", b"v".to_vec()),
    ];
    let operations = deltas.map(map_update);
    for operation in &operations {
        propose_raw(&bo_group, id, operation.clone()).await?;
    }
    group.sync().await?;

    let (pending, kept, map) = group.with_group_snapshot(|mls| {
        let pending = mls.pending_proposals().count();
        let committed = dictionary_total(
            mls.extensions()
                .app_data_dictionary()
                .map(|extension| extension.dictionary()),
        );
        // Room for the small inserts, not for the 8,000-byte value.
        let committable = Committable::pending(mls, usize::MAX, committed + 1_000)?;
        let kept: Vec<_> = committable
            .proposals
            .iter()
            .filter_map(|queued| match queued.proposal() {
                Proposal::AppDataUpdate(proposal) => Some(proposal.operation().clone()),
                _ => None,
            })
            .collect();
        let map = committable
            .states
            .get(&id)
            .and_then(|state| state.as_deref().map(|bytes| bytes.to_vec()));
        Ok((pending, kept, map))
    })?;
    // Receipt stored every proposal, so the builder judged each one.
    assert_eq!(pending, operations.len());
    let [first, _, third, _, fifth] = operations;
    assert_eq!(kept, [first, third, fifth]);
    let expected = TlsMap::from_pairs(["k0", "k1", "a", "big"].map(|key| (bytes(key), bytes("v"))));
    assert_eq!(map, Some(expected.tls_serialize_detached()?));
    Ok(())
}

/// A member floods an application map in one commit: a delta of 4,096
/// descending inserts and the ascending deletes of the same keys, the shape
/// of Octane 807b5312 far below its scale, then 999 inline one-mutation
/// Updates. Another member processes the commit with a few snapshot
/// decodes, not several per proposal, applies it, and goes on to read the
/// group's next message. This bounds decodes per commit, not the cost of one
/// delta at the finding's scale.
#[xmtp_common::test(unwrap_try = true)]
async fn test_mutation_flood_commit_is_processed_once() -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_mls_group_state::app_data::working_set::counters::snapshot_decodes;

    const ENTRIES: u16 = 2_000;
    const PROPOSALS: u16 = 1_000;
    const FLOOD: u16 = 4_096;
    /// Snapshot decodes processing a commit to one application map takes,
    /// however many proposals it carries: the registry, the membership and
    /// the map, each a few times.
    const DECODES: usize = 64;
    let id = ComponentId::new(0xC1A4);
    let alix = peer_with_application_map(id).await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    fill_map(&group, id, ENTRIES).await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    bo_group.sync().await?;
    let flood = (ENTRIES..ENTRIES + FLOOD)
        .rev()
        .fold(TlsMapDelta::new(), |delta, n| {
            delta.insert(nth_key(n), bytes(vec![]))
        });
    let flood = (ENTRIES..ENTRIES + FLOOD).fold(flood, |delta, n| delta.delete(nth_key(n)));
    let operations: Vec<_> = std::iter::once(map_update(flood))
        .chain((1..PROPOSALS).map(|n| {
            map_update(TlsMapDelta::new().update(nth_key(n % ENTRIES), bytes(n.to_le_bytes())))
        }))
        .collect();
    let epoch = group.epoch().await?;
    publish_inline(&bo_group, id, &operations).await?;

    let before = snapshot_decodes();
    group.sync().await?;
    let decodes = snapshot_decodes() - before;
    assert!(decodes <= DECODES, "{decodes} snapshot decodes");
    assert_eq!(
        group.epoch().await?,
        epoch + 1,
        "the commit was not applied"
    );

    caro_group.sync().await?;
    caro_group
        .send_message(b"after", Default::default())
        .await?;
    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs::default())?;
    assert!(
        messages
            .iter()
            .any(|message| message.decrypted_message_bytes == b"after"),
        "the member stopped processing the group"
    );
    Ok(())
}
