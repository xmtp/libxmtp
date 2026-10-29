//! Transaction boundaries and inactive groups after archive import.
use super::*;
use crate::groups::{DeleteMessageError, GroupError};
use crate::tester;
use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection};
use futures::io::{BufReader, Cursor};
use std::collections::HashMap;
use xmtp_db::{
    ConnectionExt, Store,
    consent_record::{ConsentState, ConsentType},
    group::{GroupMembershipState, GroupQueryArgs},
};
use xmtp_mls_common::group_metadata::DmMembers;
use xmtp_proto::{
    types::GroupId,
    xmtp::device_sync::group_backup::{
        GroupMembershipStateSave, GroupSave, ImmutableMetadataSave, MutableMetadataSave,
    },
};

fn saved_group(seed: u8, dm: bool) -> GroupSave {
    let pair = xmtp_mls_common::group_metadata::DmMembers {
        member_one_inbox_id: "a".repeat(64),
        member_two_inbox_id: "b".repeat(64),
    }
    .to_string();
    GroupSave {
        id: vec![seed; 16],
        created_at_ns: 123,
        membership_state: 1,
        installations_last_checked: 987,
        added_by_inbox_id: "archived-adder".into(),
        welcome_id: Some(765),
        rotated_at_ns: 456,
        conversation_type: if dm { 2 } else { 1 },
        dm_id: dm.then_some(pair),
        last_message_ns: Some(300),
        message_disappear_from_ns: Some(100),
        message_disappear_in_ns: None,
        metadata: Some(ImmutableMetadataSave {
            creator_inbox_id: "archived-creator".into(),
        }),
        mutable_metadata: Some(MutableMetadataSave {
            attributes: [
                ("group_name".into(), "historical name".into()),
                ("unknown-key".into(), "retained value".into()),
            ]
            .into(),
            admin_list: vec!["historical-admin".into(), "historical-admin".into()],
            super_admin_list: vec!["historical-super-admin".into()],
        }),
        paused_for_version: Some("99.0.0".into()),
    }
}
fn group_element(save: GroupSave) -> BackupElement {
    BackupElement {
        element: Some(Element::Group(save)),
    }
}
async fn apply(
    context: &impl XmtpSharedContext,
    elements: Vec<BackupElement>,
) -> Result<(), DeviceSyncError> {
    insert_elements(
        &mut futures::stream::iter(elements.into_iter().map(Ok::<_, std::io::Error>)),
        context,
    )
    .await
}

// verifies: ARCH-014, ARCH-020, ARCH-021
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_keeps_source_fields_and_independent_settings() {
    tester!(alix, disable_workers);
    for (index, dm) in [false, true].into_iter().enumerate() {
        let mut save = saved_group(index as u8 + 1, dm);
        if dm {
            save.metadata = None;
            save.message_disappear_from_ns = None;
            save.message_disappear_in_ns = Some(200);
        }
        apply(&alix.context, vec![group_element(save.clone())]).await?;
        let id = GroupId::try_from(save.id.as_slice())?;
        let row = alix.db().find_group(&id)??;
        assert_eq!(row.created_at_ns, 123);
        assert_eq!(row.added_by_inbox_id, "archived-adder");
        assert_eq!(row.last_message_ns, Some(300));
        assert_eq!(
            row.message_disappear_from_ns,
            save.message_disappear_from_ns
        );
        assert_eq!(row.message_disappear_in_ns, save.message_disappear_in_ns);
        assert_eq!(row.membership_state, GroupMembershipState::Restored);
        assert_eq!(row.dm_id, save.dm_id);
        assert!(!row.should_publish_commit_log);
        assert_eq!(row.sequence_id, None);
        assert_eq!(row.paused_for_version, None);
        let mut later = save.clone();
        later.created_at_ns = 999;
        later.added_by_inbox_id = "replace".into();
        later.last_message_ns = Some(500);
        later.metadata = None;
        apply(&alix.context, vec![group_element(later)]).await?;
        let after = alix.db().find_group(&id)??;
        assert_eq!(after.created_at_ns, row.created_at_ns);
        assert_eq!(after.added_by_inbox_id, row.added_by_inbox_id);
        assert_eq!(after.last_message_ns, Some(500));
        assert_eq!(after.conversation_type, row.conversation_type);
        assert_eq!(after.dm_id, row.dm_id);
        assert_eq!(
            after.message_disappear_from_ns,
            row.message_disappear_from_ns
        );
        assert_eq!(after.message_disappear_in_ns, row.message_disappear_in_ns);
    }
}

// verifies: ARCH-021
#[xmtp_common::test(unwrap_try = true)]
async fn restored_group_update_failure_rolls_back_row_stub_and_consent() {
    use xmtp_db::diesel::{connection::SimpleConnection, prelude::*};
    tester!(alix, disable_workers);
    let db = alix.db();
    let count = || {
        db.raw_query(|conn| {
            xmtp_db::schema::openmls_key_store::table
                .count()
                .get_result::<i64>(conn)
        })
    };
    let before = count()?;
    // A denied sibling of the DM gives it a record to inherit and roll back.
    let sibling = saved_group(10, true);
    apply(&alix.context, vec![group_element(sibling.clone())]).await?;
    db.insert_newer_consent_record(StoredConsentRecord {
        entity_type: ConsentType::ConversationId,
        state: ConsentState::Denied,
        entity: hex::encode(&sibling.id),
        consented_at_ns: 30,
    })?;
    db.raw_query(|conn| conn.batch_execute("CREATE TRIGGER fail_group BEFORE UPDATE OF created_at_ns ON groups BEGIN SELECT RAISE(ABORT, 'group update failure'); END;"))?;
    for (index, dm) in [false, true].into_iter().enumerate() {
        let save = saved_group(index as u8 + 3, dm);
        let id = GroupId::try_from(save.id.as_slice())?;
        assert!(
            apply(&alix.context, vec![group_element(save)])
                .await
                .is_err()
        );
        assert!(db.find_group(&id)?.is_none());
        assert!(
            db.get_consent_record(hex::encode(id), ConsentType::ConversationId)?
                .is_none()
        );
        assert_eq!(count()?, before, "the failed element left an MLS stub");
    }
    db.raw_query(|conn| conn.batch_execute("DROP TRIGGER fail_group;"))?;
    for (index, dm) in [false, true].into_iter().enumerate() {
        let save = saved_group(index as u8 + 3, dm);
        let id = GroupId::try_from(save.id.as_slice())?;
        apply(&alix.context, vec![group_element(save)]).await?;
        assert!(db.find_group(&id)?.is_some());
        let record = db.get_consent_record(hex::encode(id), ConsentType::ConversationId)?;
        assert_eq!(
            record.map(|r| (r.state, r.consented_at_ns)),
            dm.then_some((ConsentState::Denied, 30))
        );
    }
}

// verifies: ARCH-014, ARCH-021, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_completed_elements_keep_metadata_and_activity_after_failure() {
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
        Some(4),
    );
    let first = saved_group(5, false);
    let id = GroupId::try_from(first.id.as_slice())?;
    let mut descending = first.clone();
    descending.last_message_ns = Some(200);
    descending.created_at_ns = 999;
    let mut elements = futures::stream::iter([
        Ok(group_element(first.clone())),
        Ok(group_element(descending)),
        Err(std::io::Error::other("later element failed")),
    ]);
    assert!(insert_elements(&mut elements, &alix.context).await.is_err());
    let row = alix.db().find_group(&id)??;
    assert_eq!(row.last_message_ns, Some(300));
    assert_eq!(row.created_at_ns, 123);
    assert!(events.drain().iter().any(|event| matches!(
        &event.client,
        Some(ClientEvent::ArchiveRestored(ArchiveRestored {
            complete: false
        }))
    )));
    apply(&alix.context, vec![group_element(first)]).await?;
    assert!(
        events.drain().is_empty(),
        "an unchanged retry emitted another restore event"
    );
}

// verifies: ARCH-014
#[xmtp_common::test(unwrap_try = true)]
async fn restored_import_does_not_rewrite_an_existing_group() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    alix.db()
        .update_group_membership(group.group_id, GroupMembershipState::Restored)?;
    let before = alix.db().find_group(&group.group_id)??;
    let mut save = saved_group(6, false);
    save.id = group.group_id.to_vec();
    apply(&alix.context, vec![group_element(save)]).await?;
    let after = alix.db().find_group(&group.group_id)??;
    assert_eq!(after.created_at_ns, before.created_at_ns);
    assert_eq!(after.added_by_inbox_id, before.added_by_inbox_id);
}

// verifies: ARCH-014, CONS-010, DMS-010
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_dm_creation_does_not_synthesize_allowed_consent() {
    tester!(alix, disable_workers);
    for (index, existing_denied) in [false, true].into_iter().enumerate() {
        let save = saved_group(index as u8 + 7, true);
        let entity = hex::encode(&save.id);
        let denied = StoredConsentRecord {
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Denied,
            entity: entity.clone(),
            consented_at_ns: 30,
        };
        if existing_denied {
            alix.db().insert_newer_consent_record(denied.clone())?;
        }
        apply(&alix.context, vec![group_element(save.clone())]).await?;
        let record = alix
            .db()
            .get_consent_record(entity.clone(), ConsentType::ConversationId)?;
        if existing_denied {
            assert_eq!(
                record.as_ref().map(|r| (r.state, r.consented_at_ns)),
                Some((ConsentState::Denied, 30))
            );
        } else {
            assert!(record.is_none());
        }
        // Consent is applied only when the archive contains its selected element.
        let consent = BackupElement {
            element: Some(Element::Consent(denied.into())),
        };
        apply(
            &alix.context,
            vec![group_element(save.clone()), consent.clone()],
        )
        .await?;
        apply(&alix.context, vec![group_element(save), consent]).await?;
        let record = alix
            .db()
            .get_consent_record(entity, ConsentType::ConversationId)??;
        assert_eq!(
            (record.state, record.consented_at_ns),
            (ConsentState::Denied, 30)
        );
    }
    // A restored DM of the same pair without its own record inherits the
    // pair's, whether it is stored already or follows in the same archive,
    // which lists every group before any consent. A live DM keeps its own.
    tester!(cara, disable_workers);
    let denied_at = |id: &[u8], consented_at_ns| StoredConsentRecord {
        entity_type: ConsentType::ConversationId,
        state: ConsentState::Denied,
        entity: hex::encode(id),
        consented_at_ns,
    };
    let live = saved_group(11, true);
    let live_id = GroupId::try_from(live.id.as_slice())?;
    apply(&cara.context, vec![group_element(live.clone())]).await?;
    cara.db()
        .update_group_membership(live_id, GroupMembershipState::Allowed)?;
    cara.db()
        .insert_newer_consent_record(denied_at(&live.id, 20))?;
    let sibling = saved_group(10, true);
    let sibling_denied = BackupElement {
        element: Some(Element::Consent(denied_at(&sibling.id, 30).into())),
    };
    for (client, later) in [
        (&alix, vec![]),
        (
            &cara,
            vec![group_element(live), group_element(sibling), sibling_denied],
        ),
    ] {
        let save = saved_group(9, true);
        let mut elements = vec![group_element(save.clone())];
        elements.extend(later);
        apply(&client.context, elements).await?;
        let record = client
            .db()
            .get_consent_record(hex::encode(&save.id), ConsentType::ConversationId)??;
        assert_eq!(
            (record.state, record.consented_at_ns),
            (ConsentState::Denied, 30)
        );
        let listed = client.find_groups(GroupQueryArgs::default())?;
        assert!(listed.iter().all(|group| group.dm_id != save.dm_id));
    }
    let live = cara
        .db()
        .get_consent_record(hex::encode(live_id), ConsentType::ConversationId)??;
    assert_eq!(live.consented_at_ns, 20, "the restore wrote a live DM");
}

/// Opening a DM with a peer whose restored DM is denied creates a DM that
/// keeps the stored decision and its time, and is not a local decision. The
/// copy is local, so a stored time more than a day ahead is kept too.
// verifies: DMS-010, CONS-020, SYNC-021
#[rstest::rstest]
#[case::archived(false)]
#[case::stored_ahead(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_find_or_create_keeps_the_archived_denial(#[case] stored_ahead: bool) {
    use crate::subscriptions::internal::{InternalEvent, PreferenceOrigin};
    tester!(alix1, disable_workers);
    tester!(bo, disable_workers);
    let archived = alix1.find_or_create_dm(bo.inbox_id(), None).await.unwrap();
    let denied = |consented_at_ns| StoredConsentRecord {
        entity_type: ConsentType::ConversationId,
        state: ConsentState::Denied,
        entity: hex::encode(archived.group_id),
        consented_at_ns,
    };
    alix1
        .db()
        .insert_or_replace_consent_records(&[denied(30)])
        .unwrap();
    tester!(alix2, from: alix1, disable_workers);
    let mut denied_at = 30;
    if stored_ahead {
        // As an older build stored it; the archived record at 30 loses to it.
        denied_at = xmtp_common::time::now_ns() + 2 * xmtp_common::NS_IN_DAY;
        alix2
            .db()
            .insert_or_replace_consent_records(&[denied(denied_at)])
            .unwrap();
    }
    let elements = exported_elements(alix1.db()).await.unwrap();
    apply(
        &alix2.context,
        elements
            .into_iter()
            .map(|element| BackupElement {
                element: Some(element),
            })
            .collect(),
    )
    .await
    .unwrap();
    let events = alix2.context.events().subscribe(
        xmtp_events::EventFilter::new([]).with_internal(|event| {
            matches!(
                event,
                InternalEvent::PreferencesChanged {
                    origin: PreferenceOrigin::Local,
                    ..
                }
            )
        }),
        None,
    );

    let dm = alix2.find_or_create_dm(bo.inbox_id(), None).await.unwrap();
    assert_ne!(dm.group_id, archived.group_id);
    let record = alix2
        .db()
        .get_consent_record(hex::encode(dm.group_id), ConsentType::ConversationId)
        .unwrap()
        .unwrap();
    assert_eq!(
        (record.state, record.consented_at_ns),
        (ConsentState::Denied, denied_at)
    );
    let listed = alix2.find_groups(GroupQueryArgs::default()).unwrap();
    assert!(listed.iter().all(|group| group.group_id != dm.group_id));
    assert!(
        events.drain().is_empty(),
        "creation emitted a local decision"
    );
}

/// Every group element the exporter wrote, keyed by physical id.
async fn exported_groups(
    db: impl DbQuery + 'static,
) -> Result<HashMap<Vec<u8>, GroupSave>, DeviceSyncError> {
    Ok(exported_elements(db)
        .await?
        .into_iter()
        .filter_map(|element| {
            if let Element::Group(save) = element {
                Some((save.id.clone(), save))
            } else {
                None
            }
        })
        .collect())
}

async fn exported_elements(db: impl DbQuery + 'static) -> Result<Vec<Element>, DeviceSyncError> {
    let key = vec![7; 32];
    let opts = ArchiveOptions {
        start_ns: None,
        end_ns: None,
        elements: vec![
            BackupElementSelection::Messages,
            BackupElementSelection::Consent,
        ],
        exclude_disappearing_messages: false,
    };
    let mut bytes = Vec::new();
    xmtp_archive::exporter::export(opts, db, &key, &mut bytes)?;
    let mut importer =
        ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &key).await?;
    let mut elements = Vec::new();
    while let Some(element) = importer.next().await {
        if let Some(element) = element?.element {
            elements.push(element);
        }
    }
    Ok(elements)
}

// verifies: ARCH-017, ARCH-026
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_export_excludes_groups_and_messages_but_keeps_consent() {
    tester!(alix, disable_workers);
    let live = alix.create_group(None, None)?;
    let live_message = stored_message(live.group_id, alix.inbox_id().to_string(), 61);
    live_message.store(&alix.db())?;
    for (seed, dm) in [(62, false), (63, true)] {
        let save = saved_group(seed, dm);
        let id = GroupId::try_from(save.id.as_slice())?;
        apply(&alix.context, vec![group_element(save)]).await?;
        stored_message(id, alix.inbox_id().to_string(), seed).store(&alix.db())?;
        alix.db().insert_newer_consent_record(StoredConsentRecord {
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Denied,
            entity: hex::encode(id),
            consented_at_ns: 30,
        })?;
    }
    let elements = exported_elements(alix.db()).await?;
    let groups: Vec<_> = elements
        .iter()
        .filter_map(|e| match e {
            Element::Group(g) => Some(g.id.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(groups, vec![live.group_id.as_slice()]);
    let messages: Vec<_> = elements
        .iter()
        .filter_map(|e| match e {
            Element::GroupMessage(m) => Some(m.id.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(messages, vec![live_message.id.as_slice()]);
    for seed in [62, 63] {
        let entity = hex::encode([seed; 16]);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e, Element::Consent(c) if c.entity == entity))
        );
    }
}

fn stored_message(group_id: GroupId, sender_inbox_id: String, marker: u8) -> StoredGroupMessage {
    use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
    StoredGroupMessage {
        id: vec![marker; 32],
        group_id,
        decrypted_message_bytes: b"archived message".to_vec(),
        sent_at_ns: 250,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![marker; 32],
        sender_inbox_id,
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        expiry_unknown: false,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key: format!("archived-{marker}"),
    }
}

// verifies: JOIN-080, ARCH-026
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_yields_to_live_metadata_after_activation() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let mut save = saved_group(40, true);
    save.id = dm.group_id.to_vec();
    save.dm_id = Some(
        DmMembers {
            member_one_inbox_id: alix.inbox_id().to_string(),
            member_two_inbox_id: bo.inbox_id().to_string(),
        }
        .to_string(),
    );
    save.metadata = None;
    apply(&bo.context, vec![group_element(save.clone())]).await?;
    let handle = bo.group(&dm.group_id)?;
    assert_eq!(handle.added_by_inbox_id()?, "archived-adder");

    assert!(!exported_groups(bo.db()).await?.contains_key(&save.id));
    bo.sync_welcomes().await?;
    assert_ne!(
        bo.db().find_group(&dm.group_id)??.membership_state,
        GroupMembershipState::Restored
    );
    // Core caches nothing: the earlier handle and a fresh one both read live metadata.
    for group in [handle, bo.group(&dm.group_id)?] {
        assert_eq!(group.metadata().await?.creator_inbox_id, alix.inbox_id());
        assert_eq!(group.added_by_inbox_id()?, alix.inbox_id());
        assert!(group.is_active()?);
    }
    let exported = exported_groups(bo.db()).await?;
    assert_eq!(
        exported[&save.id]
            .metadata
            .as_ref()
            .map(|m| m.creator_inbox_id.as_str()),
        Some(alix.inbox_id())
    );
    assert_ne!(
        exported[&save.id].membership_state,
        GroupMembershipStateSave::from(GroupMembershipState::Restored) as i32
    );
}

// verifies: ARCH-015
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_never_grants_authority() {
    tester!(alix, disable_workers);
    let me = alix.inbox_id().to_string();
    let mut save = saved_group(50, false);
    save.metadata = Some(ImmutableMetadataSave {
        creator_inbox_id: me.clone(),
    });
    let lists = save.mutable_metadata.as_mut()?;
    lists.admin_list = vec![me.clone()];
    lists.super_admin_list = vec![me.clone()];
    apply(&alix.context, vec![group_element(save.clone())]).await?;
    let id = GroupId::try_from(save.id.as_slice())?;
    let message = stored_message(id, "a".repeat(64), 0x61);
    message.store(&alix.db())?;
    let group = alix.group(&id)?;

    // The imported message remains readable.
    assert!(group.metadata().await?.creator_inbox_id.is_empty());
    assert_eq!(
        alix.db().get_group_message(&message.id)?.map(|m| m.id),
        Some(message.id.clone())
    );

    // A forged claim authorizes nothing before activation.
    assert!(matches!(
        group.delete_message(message.id.clone()).unwrap_err(),
        GroupError::DeleteMessage(DeleteMessageError::NotAuthorized)
    ));
    assert!(alix.db().get_group_message(&message.id)?.is_some());
    assert!(matches!(
        group
            .send_message(b"forged", Default::default())
            .await
            .unwrap_err(),
        GroupError::GroupInactive
    ));
    assert!(group.update_group_name("forged".into()).await.is_err());
    assert_eq!(group.group_name()?, "historical name");
}

// verifies: ARCH-021
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_ignores_a_stray_dm_id_on_a_group() {
    use xmtp_db::group::ConversationType;
    tester!(alix, disable_workers);
    let mut save = saved_group(60, false);
    save.dm_id = Some("not-a-pair".into());
    apply(&alix.context, vec![group_element(save.clone())]).await?;
    let id = GroupId::try_from(save.id.as_slice())?;
    assert_eq!(alix.db().find_group(&id)??.dm_id, None);
    let metadata = alix.group(&id)?.metadata().await?;
    assert_eq!(metadata.conversation_type, ConversationType::Group);
    assert_eq!(metadata.dm_members, None);
}

// verifies: ARCH-015, JOIN-080, PERM-001
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_forged_claim_yields_to_live_permissions_after_activation() {
    use crate::groups::{
        GroupError, UpdateAdminListType, mls_sync::GroupMessageProcessingError,
        validated_commit::CommitValidationError,
    };
    use xmtp_mls_validation::commit::CommitRuleError;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let me = bo.inbox_id().to_string();
    let mut save = saved_group(80, false);
    save.id = alix_group.group_id.to_vec();
    save.metadata = Some(ImmutableMetadataSave {
        creator_inbox_id: me.clone(),
    });
    let lists = save.mutable_metadata.as_mut()?;
    lists.admin_list = vec![me.clone()];
    lists.super_admin_list = vec![me.clone()];
    // The archive arrives before the Welcome: a Restored row with forged claims.
    apply(&bo.context, vec![group_element(save)]).await?;
    let handle = bo.group(&alix_group.group_id)?;
    assert!(handle.metadata().await?.creator_inbox_id.is_empty());

    bo.sync_welcomes().await?;
    assert_ne!(
        bo.db().find_group(&alix_group.group_id)??.membership_state,
        GroupMembershipState::Restored
    );
    for group in [handle, bo.group(&alix_group.group_id)?] {
        assert_eq!(group.metadata().await?.creator_inbox_id, alix.inbox_id());
        assert!(!group.is_super_admin(me.clone())?);
        assert!(!group.is_admin(me.clone())?);
        assert!(group.is_super_admin(alix.inbox_id().to_string())?);
        // The committed group context decides: the archived claim grants nothing.
        let error = group
            .update_admin_list(UpdateAdminListType::AddSuper, me.clone())
            .await
            .expect_err("the archived super-admin claim grants nothing");
        let GroupError::Sync(summary) = error else {
            panic!("expected a rejected commit, got {error:?}");
        };
        assert!(
            matches!(
                summary.other.as_deref(),
                Some(GroupError::ReceiveError(
                    GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                        CommitRuleError::InsufficientPermissions
                    ))
                ))
            ),
            "{summary:?}"
        );
        // The same handle holds the live membership right to send.
        group
            .send_message(b"after activation", Default::default())
            .await?;
    }
    alix_group.sync().await?;
    assert_eq!(alix_group.super_admin_list()?, [alix.inbox_id()]);
    assert_eq!(
        bo.group(&alix_group.group_id)?.super_admin_list()?,
        [alix.inbox_id()]
    );
}

/// A Welcome activates a restored placeholder only when its root names the
/// group. A placeholder holds no root. A 16-byte placeholder stays Restored
/// under every Welcome, a 32-byte placeholder stays Restored under a Welcome
/// from another root, and the real group's Welcome activates its placeholder.
// verifies: JOIN-080, JOIN-083, ARCH-015
#[xmtp_common::test(unwrap_try = true)]
async fn restored_placeholder_activates_only_for_its_root() {
    use crate::utils::test::crafted_welcome::send_crafted_welcome;
    use xmtp_mls_common::{
        app_data::{component_id::ComponentId, group_root::read_group_root},
        group_keys::GroupRoot,
    };
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let real = alix.create_group(None, None)?;
    let legacy = saved_group(90, false);
    let mut foreign = saved_group(91, false);
    foreign.id = xmtp_common::rand_vec::<32>();
    let mut activated = saved_group(92, false);
    activated.id = real.group_id.to_vec();
    apply(
        &bo.context,
        [&legacy, &foreign, &activated]
            .into_iter()
            .map(|save| group_element(save.clone()))
            .collect(),
    )
    .await?;
    let id = |save: &GroupSave| GroupId::try_from(save.id.as_slice()).unwrap();
    let state = |save: &GroupSave| {
        bo.db()
            .find_group(&id(save))
            .unwrap()
            .unwrap()
            .membership_state
    };
    for save in [&legacy, &foreign, &activated] {
        let (root, signer) = bo.group(&id(save))?.with_group_snapshot(|mls_group| {
            let extensions = mls_group.extensions();
            Ok((
                read_group_root(extensions).map(|root| root.is_some()),
                extensions.app_data_dictionary().is_some_and(|d| {
                    d.dictionary()
                        .contains(&ComponentId::COMMIT_LOG_SIGNER.as_u16())
                }),
            ))
        })?;
        assert_eq!(root, Ok(false), "a placeholder holds no root");
        assert!(!signer, "a placeholder holds no signer");
    }

    let other_root = GroupRoot::generate();
    send_crafted_welcome(&alix, &bo, &legacy.id, None).await;
    send_crafted_welcome(&alix, &bo, &legacy.id, Some(&other_root)).await;
    send_crafted_welcome(&alix, &bo, &foreign.id, Some(&other_root)).await;
    assert!(bo.sync_welcomes().await?.is_empty());
    for save in [&legacy, &foreign, &activated] {
        assert_eq!(state(save), GroupMembershipState::Restored);
    }

    real.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    assert_ne!(state(&activated), GroupMembershipState::Restored);
    for save in [&legacy, &foreign] {
        assert_eq!(state(save), GroupMembershipState::Restored);
    }
}
