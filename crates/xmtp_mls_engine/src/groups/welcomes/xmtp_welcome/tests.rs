use super::*;
use crate::groups::InitialMembershipValidator;
use crate::groups::test_support::MlsGroupExt;
use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestContext, EngineTestPeer, EngineTestPeerOps,
};
use crate::prelude::*;

struct UnavailableValidator;

impl ValidateGroupMembership for UnavailableValidator {
    async fn check_initial_membership(
        &self,
        _welcome: &WelcomeMembership,
    ) -> Result<(), GroupError> {
        Err(GroupError::LockUnavailable)
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn trial_validation_failure_preserves_welcome_keys() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.invite(&bo).await?;
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await?
        .pop()?;

    let events = bo.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationJoined]),
        Some(10),
    );
    let result = XmtpWelcome::builder()
        .context(bo.context.clone())
        .welcome(&welcome)
        .pending(
            crate::groups::welcome_sync::pending_welcome_for_test(&bo.context, &welcome).await?,
        )
        .validator(UnavailableValidator)
        .process()
        .await;
    assert!(matches!(result, Err(GroupError::LockUnavailable)));
    assert!(events.drain().is_empty());
    assert!(bo.context.db().find_group(&alix_group.group_id)?.is_none());
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(bo.context.installation_id(), EntityKind::Welcome)?,
        Cursor(0)
    );

    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert!(matches!(
        events.drain().pop().and_then(|event| event.client),
        Some(xmtp_events::ClientEvent::ConversationJoined(joined)) if joined.group_id == bo_group.group_id.to_vec()
    ));
    alix_group.test_can_talk_with(&bo_group).await?;
}

struct JoinDuringValidation<C> {
    context: C,
    welcome: xmtp_proto::types::WelcomeMessage,
}

impl<C: XmtpSharedContext> ValidateGroupMembership for JoinDuringValidation<C> {
    async fn check_initial_membership(
        &self,
        _welcome: &WelcomeMembership,
    ) -> Result<(), GroupError> {
        let group = XmtpWelcome::builder()
            .context(self.context.clone())
            .welcome(&self.welcome)
            .pending(
                crate::groups::welcome_sync::pending_welcome_for_test(&self.context, &self.welcome)
                    .await?,
            )
            .validator(InitialMembershipValidator::new(self.context.clone()))
            .process()
            .await?
            .ok_or(GroupError::UninitializedResult)?;
        group.sync_with_conn().await?;
        Ok(())
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn fresh_install_rejects_welcome_after_another_writer_advances_group() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.invite(&bo).await?;
    alix_group
        .update_group_name("advanced during validation".into())
        .await?;
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await?
        .pop()?;

    let result = XmtpWelcome::builder()
        .context(bo.context.clone())
        .welcome(&welcome)
        .pending(
            crate::groups::welcome_sync::pending_welcome_for_test(&bo.context, &welcome).await?,
        )
        .validator(JoinDuringValidation {
            context: bo.context.clone(),
            welcome: welcome.clone(),
        })
        .process()
        .await;
    assert!(matches!(
        result,
        Err(GroupError::ProcessIntent(
            ProcessIntentError::WelcomeAlreadyProcessed(_)
        ))
    ));
    let bo_group = bo.group(&alix_group.group_id)?;
    assert_eq!(
        alix_group.epoch_authenticator().await?,
        bo_group.epoch_authenticator().await?
    );
    alix_group.test_can_talk_with(&bo_group).await?;
}

/// Process `joiner`'s newest Welcome with the production validator.
async fn process_newest_welcome(
    joiner: &EngineTestPeer,
) -> Result<Option<MlsGroup<EngineTestContext>>, GroupError> {
    let welcome = joiner
        .context
        .api()
        .query_welcome_messages(joiner.context.installation_id())
        .await?
        .into_iter()
        .max_by_key(|welcome| welcome.cursor)
        .ok_or(GroupError::UninitializedResult)?;
    XmtpWelcome::builder()
        .context(joiner.context.clone())
        .welcome(&welcome)
        .pending(
            crate::groups::welcome_sync::pending_welcome_for_test(&joiner.context, &welcome)
                .await?,
        )
        .validator(InitialMembershipValidator::new(joiner.context.clone()))
        .process()
        .await
}

/// A Welcome whose group context has no root, a root for another id, or a
/// 16-byte group id is rejected for good, and its group is not installed.
/// A one-shot Welcome with no root is rejected the same way, before its
/// one-shot message is read. The same crafted Welcome with the group's own
/// root joins, so the root is the only reason for each rejection.
// verifies: JOIN-083, JOIN-048
#[xmtp_common::test(unwrap_try = true)]
async fn a_welcome_joins_only_when_its_root_names_the_group() {
    use crate::groups::test_support::crafted_welcome::{
        assert_welcome_rejected, send_crafted_welcome, send_crafted_welcome_of_type,
    };
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};
    use xmtp_mls_group_state::app_data::group_root::GroupRootError;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let keyed_id = |root: &GroupRoot| GroupKeys::derive(root).group_id().to_vec();
    let root = GroupRoot::generate();
    let other_root = GroupRoot::generate();
    let legacy_id = xmtp_common::rand_vec::<{ xmtp_configuration::LEGACY_GROUP_ID_BYTES }>();

    for (case, group_id, welcome_root, conversation_type, expected) in [
        (
            "no root",
            keyed_id(&root),
            None,
            ConversationType::Group,
            GroupRootError::Missing,
        ),
        (
            "another group's root",
            keyed_id(&root),
            Some(&other_root),
            ConversationType::Group,
            GroupRootError::OtherGroup,
        ),
        (
            "a 16-byte group",
            legacy_id.clone(),
            Some(&root),
            ConversationType::Group,
            GroupRootError::OtherGroup,
        ),
        (
            "a one-shot group with no root",
            keyed_id(&other_root),
            None,
            ConversationType::Oneshot,
            GroupRootError::Missing,
        ),
    ] {
        send_crafted_welcome_of_type(&alix, &bo, &group_id, welcome_root, conversation_type).await;
        let cursor = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await?
            .into_iter()
            .map(|welcome| welcome.cursor)
            .max()
            .expect(case);
        let result = process_newest_welcome(&bo).await;
        assert!(
            matches!(&result, Err(GroupError::WelcomeGroupRoot(error)) if *error == expected),
            "{case}: {result:?}"
        );
        let group_id = GroupId::try_from(group_id.as_slice())?;
        assert_eq!(
            assert_welcome_rejected(&bo, &group_id),
            cursor,
            "{case}: the rejection is this Welcome's"
        );
    }

    let joined_id = keyed_id(&root);
    send_crafted_welcome(&alix, &bo, &joined_id, Some(&root)).await;
    let joined = process_newest_welcome(&bo).await?.expect("a group");
    assert_eq!(joined.group_id.as_slice(), joined_id.as_slice());
}

/// An own installation that has not loaded its revocation offers a sync
/// group or a group. The joiner holds the revocation, so it rejects the
/// Welcome for good and stores neither the group nor a consent record. A
/// current own adder still joins while the newest own state is uncached.
// verifies: JOIN-095
#[rstest::rstest]
#[case::revoked_sync(ConversationType::Sync, true)]
#[case::revoked_group(ConversationType::Group, true)]
#[case::current_uncached(ConversationType::Group, false)]
#[xmtp_common::test(unwrap_try = true)]
async fn own_adder_must_be_current(#[case] kind: ConversationType, #[case] revoked: bool) {
    use crate::groups::test_support::crafted_welcome::{
        assert_welcome_rejected, send_crafted_welcome_of_type,
    };
    use diesel::prelude::*;
    use xmtp_db::{ConnectionExt, schema::association_state::dsl};
    use xmtp_id::associations::test_utils::add_wallet_signature;
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};

    let alix1 = EngineTestPeer::new().await;
    let alix2 = alix1.new_installation().await;
    let identity = alix1.identity_updates();
    let db = alix1.context.db();
    if revoked {
        let revoked = vec![alix2.context.installation_id().to_vec()];
        let mut revoke = identity.revoke_installations(revoked).await.unwrap();
        add_wallet_signature(&mut revoke, &alix1.owner).await;
        identity.apply_signature_request(revoke).await.unwrap();
    } else {
        // A newer own update is stored, but its state is not cached.
        let _alix3 = alix1.new_installation().await;
        let inbox_id = alix1.inbox_id();
        identity
            .get_latest_association_state(&db, inbox_id)
            .await
            .unwrap();
        let newest = db.get_latest_sequence_id_for_inbox(inbox_id).unwrap();
        let cached = dsl::association_state.filter(dsl::sequence_id.eq(newest));
        db.raw_query(|conn| diesel::delete(cached).execute(conn))
            .unwrap();
    }

    let root = GroupRoot::generate();
    let group_id = GroupKeys::derive(&root).group_id().to_vec();
    send_crafted_welcome_of_type(&alix2, &alix1, &group_id, Some(&root), kind).await;
    let group_id = GroupId::try_from(group_id.as_slice()).unwrap();
    if !revoked {
        // The Welcome can take a moment to become readable.
        let joined = xmtp_common::wait_for_some(|| async {
            alix1.sync_welcomes().await.unwrap();
            db.find_group(&group_id).unwrap()
        });
        assert!(joined.await.is_some());
        return;
    }

    let result = process_newest_welcome(&alix1).await;
    assert!(
        matches!(result, Err(GroupError::InvalidGroupMembership)),
        "{result:?}"
    );
    assert_welcome_rejected(&alix1, &group_id);
    let consent = db
        .consent_row(&xmtp_db::consent_record::ConsentKey::group(&group_id))
        .unwrap();
    assert!(consent.is_none(), "{consent:?}");
}

/// A former member can offer a second MLS branch under the group's root,
/// at a lower epoch than the removed copy and with an anchor past the
/// removal. It is rejected for good, and the removed copy keeps its
/// epoch, its cursor, and the envelopes queued after the removal.
// verifies: JOIN-042
#[xmtp_common::test(unwrap_try = true)]
async fn inactive_group_rejects_stale_branch_and_keeps_pending_prefix() {
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_db::incoming_envelope::QueryIncomingEnvelope;
    use xmtp_mls_group_state::app_data::group_root::read_group_root;
    use xmtp_proto::types::Topic;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    let root = read_group_root(alix_group.load_group_context()?.extensions())?
        .expect("a new group carries its root");
    alix_group.invite(&bo).await?;
    let bo_group = bo.sync_welcomes().await?.pop().expect("initial Welcome");
    bo_group.sync().await?;
    alix_group.remove_members(&[bo.inbox_id()]).await?;
    let _ = bo_group.sync().await;
    assert!(!bo_group.is_active()?, "the removal must be applied");

    let db = bo.context.db();
    let topic = StreamTopic::group(bo_group.group_id);
    let before = db.topic_progress(&topic)?;
    let old_epoch = bo_group.epoch().await?;
    assert!(old_epoch > 1, "the crafted branch joins at epoch one");

    // Receive a real group envelope after the removal without processing
    // it. The forged anchor names this pending network position.
    alix_group.send_msg(b"after removal").await;
    bo.mls_store()
        .receive_topics_once(
            &[Topic::new_group_message(bo_group.group_id)],
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(topic.kind),
        )
        .await?;
    let forged_anchor = db.topic_progress(&topic)?.received;
    assert!(before.processed < forged_anchor);
    assert!(db.pending_envelope(&topic, forged_anchor)?.is_some());

    let options = CraftedWelcome {
        anchor: forged_anchor.0,
        ..Default::default()
    };
    send_crafted_welcome_with(
        &alix,
        &bo,
        alix_group.group_id.as_slice(),
        Some(&root),
        options,
    )
    .await;
    let result = process_newest_welcome(&bo).await;
    assert!(
        matches!(
            result,
            Err(GroupError::ProcessIntent(
                ProcessIntentError::WelcomeAlreadyProcessed(_)
            ))
        ),
        "{result:?}"
    );
    bo.assert_newest_welcome_rejected().await;
    send_crafted_welcome_with(
        &alix,
        &bo,
        alix_group.group_id.as_slice(),
        Some(&root),
        CraftedWelcome {
            anchor: before.processed.0,
            prior_epoch: old_epoch - 1,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(
        process_newest_welcome(&bo).await,
        Err(GroupError::ProcessIntent(
            ProcessIntentError::WelcomeAlreadyProcessed(_)
        ))
    ));
    bo.assert_newest_welcome_rejected().await;
    assert!(!bo_group.is_active()?);
    assert_eq!(bo_group.epoch().await?, old_epoch);
    assert_eq!(db.topic_progress(&topic)?.processed, before.processed);
    assert!(db.pending_envelope(&topic, forged_anchor)?.is_some());
}

/// A removed former member reuses the group's root to offer a DM, then a
/// one-shot group, under the group's id. Each Welcome is rejected for good
/// before anything is installed or any one-shot handling runs: the stored
/// row, MLS state, and cursor stay as they were.
// verifies: JOIN-089
#[xmtp_common::test(unwrap_try = true)]
async fn replacement_welcome_cannot_change_conversation_type() {
    use crate::groups::test_support::crafted_welcome::{
        CraftedWelcome, send_crafted_welcome_of_type, send_crafted_welcome_with,
    };
    use xmtp_mls_group_state::app_data::group_root::read_group_root;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    let original = alix.create_group(None, None)?;
    original.invite(&caro).await?;
    let caro_group = caro.sync_welcomes().await?.pop().expect("Caro joined");
    let retained_root = read_group_root(caro_group.load_group_context()?.extensions())?
        .expect("the original group has a root");
    original.invite(&bo).await?;
    let bo_group = bo.sync_welcomes().await?.pop().expect("Bo joined");
    original.remove_members(&[caro.inbox_id()]).await?;
    original.remove_members(&[bo.inbox_id()]).await?;
    bo_group.sync_with_conn().await?;
    assert!(!bo_group.is_active()?);
    let row_before_forgery = bo
        .context
        .db()
        .find_group(&original.group_id)?
        .expect("stored");
    assert_eq!(
        row_before_forgery.conversation_type,
        ConversationType::Group
    );
    let anchor = bo_group.cursor().await?;
    let prior_epoch = bo_group.epoch().await?;

    // Caro's DM with Bo, at the next epoch and anchored at Bo's cursor,
    // passes every check a same-type re-add Welcome would.
    let options = CraftedWelcome {
        conversation_type: ConversationType::Dm,
        anchor: anchor.0,
        prior_epoch,
        ..Default::default()
    };
    send_crafted_welcome_with(
        &caro,
        &bo,
        original.group_id.as_slice(),
        Some(&retained_root),
        options,
    )
    .await;
    let result = process_newest_welcome(&bo).await;
    assert!(
        matches!(result, Err(GroupError::InvalidWelcomeMetadata)),
        "{result:?}"
    );
    bo.assert_newest_welcome_rejected().await;

    send_crafted_welcome_of_type(
        &caro,
        &bo,
        original.group_id.as_slice(),
        Some(&retained_root),
        ConversationType::Oneshot,
    )
    .await;
    let result = process_newest_welcome(&bo).await;
    assert!(
        matches!(result, Err(GroupError::InvalidWelcomeMetadata)),
        "{result:?}"
    );
    bo.assert_newest_welcome_rejected().await;

    assert_eq!(
        bo.context.db().find_group(&original.group_id)?,
        Some(row_before_forgery)
    );
    assert!(!bo_group.is_active()?);
    assert_eq!(bo_group.epoch().await?, prior_epoch);
    assert_eq!(bo_group.cursor().await?, anchor);
}

/// A member of a live DM holds its root and can offer a DM under its id
/// with another pair, at the next epoch and past the DM's cursor, which
/// would otherwise be held for the DM's topic. The client rejects it for
/// good at once and keeps the DM it holds.
// verifies: JOIN-089
#[xmtp_common::test(unwrap_try = true)]
async fn live_dm_rejects_welcome_with_another_pair() {
    use crate::groups::DmValidationError;
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_mls_group_state::app_data::group_root::read_group_root;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.sync_welcomes().await?.pop().expect("Bo joined the DM");
    assert!(bo_dm.is_active()?);
    let root = read_group_root(alix_dm.load_group_context()?.extensions())?
        .expect("a new DM carries its root");
    let row_before = bo.context.db().find_group(&alix_dm.group_id)?;
    let cursor_before = bo_dm.cursor().await?;
    let epoch_before = bo_dm.epoch().await?;

    let options = CraftedWelcome {
        conversation_type: ConversationType::Dm,
        anchor: cursor_before.0 + 1,
        prior_epoch: epoch_before,
        dm_peer: Some(caro.inbox_id()),
        ..Default::default()
    };
    send_crafted_welcome_with(
        &alix,
        &bo,
        alix_dm.group_id.as_slice(),
        Some(&root),
        options,
    )
    .await;
    let result = process_newest_welcome(&bo).await;
    assert!(
        matches!(
            result,
            Err(GroupError::MetadataPermissionsError(
                MetadataPermissionsError::DmValidation(DmValidationError::StoredDmIdMismatch)
            ))
        ),
        "{result:?}"
    );
    bo.assert_newest_welcome_rejected().await;
    assert_eq!(bo.context.db().find_group(&alix_dm.group_id)?, row_before);
    assert_eq!(bo_dm.cursor().await?, cursor_before);
    assert_eq!(bo_dm.epoch().await?, epoch_before);
    alix_dm.test_can_talk_with(&bo_dm).await?;
}

/// A DM Welcome whose registry differs from the fixed DM tables, here in
/// a `GROUP_MEMBERSHIP` update policy of deny, is rejected for good and
/// the DM never appears. The same crafted Welcome with the creator's
/// registry joins, so the registry is the only reason.
// verifies: DMS-021, JOIN-060
#[xmtp_common::test(unwrap_try = true)]
async fn dm_welcome_with_a_nonstandard_registry_is_rejected() {
    use crate::groups::DmValidationError;
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};
    use xmtp_mls_group_state::app_data::{
        component_id::ComponentId, dm_registry_profile::dm_registry_profiles,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind, MetadataBasePolicy},
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let mut registry = dm_registry_profiles().pop()?;
    let mut membership = registry.get(&ComponentId::GROUP_MEMBERSHIP)?.unwrap();
    membership.permissions.as_mut().unwrap().update_policy = Some(MetadataPolicy {
        kind: Some(Kind::Base(MetadataBasePolicy::Deny as i32)),
    });
    registry.set(ComponentId::GROUP_MEMBERSHIP, membership)?;

    let crafted_dm = async |registry| {
        let root = GroupRoot::generate();
        let group_id = GroupKeys::derive(&root).group_id().to_vec();
        let options = CraftedWelcome {
            conversation_type: ConversationType::Dm,
            registry,
            ..Default::default()
        };
        send_crafted_welcome_with(&alix, &bo, &group_id, Some(&root), options).await;
        group_id
    };

    let rejected_id = crafted_dm(Some(&registry)).await;
    let result = process_newest_welcome(&bo).await;
    assert!(
        matches!(
            result,
            Err(GroupError::MetadataPermissionsError(
                MetadataPermissionsError::DmValidation(DmValidationError::InvalidPermissions)
            ))
        ),
        "{result:?}"
    );
    bo.assert_newest_welcome_rejected().await;
    let rejected_id = GroupId::try_from(rejected_id.as_slice())?;
    assert!(bo.context.db().find_group(&rejected_id)?.is_none());

    let joined_id = crafted_dm(None).await;
    let joined = process_newest_welcome(&bo)
        .await?
        .expect("the creator's registry joins");
    assert_eq!(joined.group_id.as_slice(), joined_id.as_slice());
}

/// Another inbox can add this installation to a group whose creator field
/// names this inbox. The join keeps a decision that reached this
/// installation first, and stores no decision when there was none.
// verifies: CONS-058
#[rstest::rstest]
#[case::prior_denied(true)]
#[case::no_record(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn forged_creator_claim_keeps_prior_consent(#[case] prior_denied: bool) {
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentType, StoredConsentRecord};
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let root = GroupRoot::generate();
    let group_id = GroupKeys::derive(&root).group_id().to_vec();
    let entity = hex::encode(&group_id);
    // A peer installation may sync a decision before this Welcome.
    let prior = prior_denied.then(|| {
        StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Denied,
            entity.clone(),
        )
    });
    if let Some(prior) = &prior {
        prior.merge_into(&bo.context.db()).unwrap();
    }

    let options = CraftedWelcome {
        claimed_creator: Some(bo.inbox_id()),
        ..Default::default()
    };
    send_crafted_welcome_with(&alix, &bo, &group_id, Some(&root), options).await;
    let joined = process_newest_welcome(&bo)
        .await
        .unwrap()
        .expect("valid Welcome");
    assert_eq!(joined.added_by_inbox_id().unwrap(), alix.inbox_id());
    let stored = bo
        .context
        .db()
        .consent_row(&ConsentKey::group(&group_id))
        .unwrap();
    assert_eq!(
        stored.map(|record| (record.state, record.consented_at_ns)),
        prior.map(|record| (record.state, record.consented_at_ns))
    );
}

/// No join stores consent: a stranger's group that names this inbox as
/// its creator, joined by one installation from the stranger and by
/// another from the first, reads unknown on both, and a group this inbox
/// created, joined by another of its installations, gets no record there.
// verifies: CONS-058
#[xmtp_common::test(unwrap_try = true)]
async fn joining_stores_no_state() {
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_db::consent_record::{ConsentKey, ConsentState};
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};

    let alix = EngineTestPeer::new().await;
    let bo1 = EngineTestPeer::new().await;
    let bo2 = bo1.new_installation().await;
    let root = GroupRoot::generate();
    let group_id = GroupKeys::derive(&root).group_id().to_vec();
    let options = CraftedWelcome {
        claimed_creator: Some(bo1.inbox_id()),
        ..Default::default()
    };
    send_crafted_welcome_with(&alix, &bo1, &group_id, Some(&root), options).await;
    let joined = process_newest_welcome(&bo1).await?.expect("valid Welcome");
    // bo1 adds its other installation, as a member may.
    joined.update_installations().await?;
    bo2.sync_welcomes().await?;
    let own = bo1.create_group(None, None)?;
    own.update_installations().await?;
    bo2.sync_welcomes().await?;
    for bo in [&bo1, &bo2] {
        assert_eq!(
            bo.group(&joined.group_id)?.consent_state()?,
            ConsentState::Unknown
        );
        assert!(
            bo.db()
                .consent_row(&ConsentKey::group(&group_id))?
                .is_none()
        );
    }
    assert_eq!(
        bo1.group(&own.group_id)?.consent_state()?,
        ConsentState::Allowed
    );
    let own_key = ConsentKey::group(own.group_id.as_ref());
    assert!(
        bo2.db().consent_row(&own_key)?.is_none(),
        "the join stored nothing"
    );
}

/// A former member of group G holds G's root and can build a DM under
/// G's id. The DM reads its builder's inbox record; the user's decision
/// about G stays under G's id and nothing is moved or owed.
// verifies: CONS-052, CONS-058
#[xmtp_common::test(unwrap_try = true)]
async fn forged_dm_with_group_id_reads_its_builder() {
    use crate::groups::test_support::crafted_welcome::{CraftedWelcome, send_crafted_welcome_with};
    use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentType, StoredConsentRecord};
    use xmtp_mls_group_state::app_data::group_root::read_group_root;

    let alix = EngineTestPeer::new().await;
    let bo2 = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let root = read_group_root(group.load_group_context()?.extensions())?
        .expect("a new group carries its root");
    // bo2 holds the user's allow of G from sync, but not G.
    let g = ConsentKey::group(group.group_id.as_ref());
    StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        g.entity.clone(),
    )
    .merge_into(&bo2.db())?;
    let options = CraftedWelcome {
        conversation_type: ConversationType::Dm,
        ..Default::default()
    };
    send_crafted_welcome_with(&alix, &bo2, group.group_id.as_slice(), Some(&root), options).await;
    let dm = process_newest_welcome(&bo2)
        .await?
        .expect("the forged DM joins");
    assert_eq!(dm.group_id, group.group_id);
    assert_eq!(dm.consent_state()?, ConsentState::Unknown);
    assert_eq!(
        bo2.db().consent_row(&g)?.map(|r| r.state),
        Some(ConsentState::Allowed)
    );
    assert!(
        bo2.db()
            .consent_row(&ConsentKey::inbox(alix.inbox_id()))?
            .is_none()
    );
    assert!(!xmtp_db::sync_outbox::has_owed(&bo2.db())?);
}

// verifies: JOIN-044
#[rstest::rstest]
#[case::late_active_prefix(false)]
#[case::live_retired_controller(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn rejoin_keeps_messages_before_removal(#[case] live_receiver: bool) {
    use crate::subscriptions::incoming::{
        IncomingCoordinator, IncomingRegistration, IncomingScope,
    };
    use xmtp_common::time::{Duration, timeout};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.invite(&bo).await.unwrap();
    let bo_group = bo.sync_welcomes().await.unwrap().pop().unwrap();
    let lease = live_receiver
        .then(|| IncomingCoordinator::for_context(&bo.context).acquire(IncomingScope::AllGroups));
    let topic = xmtp_proto::types::Topic::new_group_message(bo_group.group_id);
    alix_group.send_msg(b"before removal").await;
    alix_group.remove_members(&[bo.inbox_id()]).await.unwrap();
    if let Some(lease) = &lease {
        timeout(Duration::from_secs(10), async {
            loop {
                if lease.snapshot().topics.iter().any(|entry| {
                    entry.topic == topic && entry.registration == IncomingRegistration::Removed
                }) {
                    break;
                }
                lease.changed().await;
            }
        })
        .await
        .unwrap();
    }
    alix_group.invite(&bo).await.unwrap();

    bo.sync_welcomes().await.unwrap();
    let messages = bo
        .context
        .db()
        .get_group_messages(&bo_group.group_id, &Default::default())
        .unwrap();
    assert!(
        messages
            .iter()
            .any(|message| message.decrypted_message_bytes == b"before removal")
    );
    assert_eq!(
        alix_group.epoch_authenticator().await.unwrap(),
        bo_group.epoch_authenticator().await.unwrap()
    );
    if let Some(lease) = &lease {
        alix_group.send_msg(b"after rejoin").await;
        timeout(Duration::from_secs(10), async {
            loop {
                let messages = bo
                    .context
                    .db()
                    .get_group_messages(&bo_group.group_id, &Default::default())
                    .unwrap();
                if messages
                    .iter()
                    .any(|message| message.decrypted_message_bytes == b"after rejoin")
                {
                    return Ok::<_, xmtp_db::StorageError>(());
                }
                lease.changed().await;
            }
        })
        .await
        .unwrap()
        .unwrap();
    }
    alix_group.test_can_talk_with(&bo_group).await.unwrap();
}
