//! Transaction boundaries and historical projections for restored history.
use super::*;
use crate::groups::DeleteMessageError;
use crate::tester;
use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection};
use futures::{
    AsyncReadExt,
    io::{BufReader, Cursor},
};
use prost::Message;
use std::collections::HashMap;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_db::{
    ConnectionExt, Store,
    consent_record::{ConsentState, ConsentType},
    group::GroupMembershipState,
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
        let history = alix.db().restored_group_metadata(&id)??;
        assert_eq!(GroupSave::decode(history.group_save.as_slice())?, save);
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
        assert_eq!(alix.db().restored_group_metadata(&id)??, history);
    }
}

// verifies: ARCH-021
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_insert_failure_rolls_back_row_stub_and_consent() {
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
    db.raw_query(|conn| conn.batch_execute("CREATE TRIGGER fail_history BEFORE INSERT ON restored_group_metadata BEGIN SELECT RAISE(ABORT, 'history failure'); END;"))?;
    for (index, dm) in [false, true].into_iter().enumerate() {
        let save = saved_group(index as u8 + 3, dm);
        let id = GroupId::try_from(save.id.as_slice())?;
        assert!(
            apply(&alix.context, vec![group_element(save)])
                .await
                .is_err()
        );
        assert!(db.find_group(&id)?.is_none());
        assert!(db.restored_group_metadata(&id)?.is_none());
        assert!(
            db.get_consent_record(hex::encode(id), ConsentType::ConversationId)?
                .is_none()
        );
        assert_eq!(count()?, before, "the failed element left an MLS stub");
    }
    db.raw_query(|conn| conn.batch_execute("DROP TRIGGER fail_history;"))?;
    for (index, dm) in [false, true].into_iter().enumerate() {
        let save = saved_group(index as u8 + 3, dm);
        let id = GroupId::try_from(save.id.as_slice())?;
        apply(&alix.context, vec![group_element(save)]).await?;
        assert!(db.find_group(&id)?.is_some());
        assert!(db.restored_group_metadata(&id)?.is_some());
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
    assert_eq!(
        GroupSave::decode(
            alix.db()
                .restored_group_metadata(&id)??
                .group_save
                .as_slice()
        )?,
        first
    );
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

// verifies: ARCH-014, ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_does_not_rewrite_an_old_row_without_a_blob() {
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
    assert!(
        alix.db()
            .restored_group_metadata(&group.group_id)?
            .is_none()
    );
}

// verifies: ARCH-014, CONS-010
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
}

/// Every group element the exporter wrote, keyed by physical id.
async fn exported_groups(
    db: impl DbQuery + 'static,
) -> Result<HashMap<Vec<u8>, GroupSave>, DeviceSyncError> {
    let key = vec![7; 32];
    let opts = ArchiveOptions {
        start_ns: None,
        end_ns: None,
        elements: vec![BackupElementSelection::Messages],
        exclude_disappearing_messages: false,
    };
    let mut bytes = Vec::new();
    ArchiveExporter::new(opts, db, &key)
        .read_to_end(&mut bytes)
        .await?;
    let mut importer =
        ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &key).await?;
    let mut saves = HashMap::new();
    while let Some(element) = importer.next().await {
        if let Some(Element::Group(save)) = element?.element {
            saves.insert(save.id.clone(), save);
        }
    }
    Ok(saves)
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
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key: format!("archived-{marker}"),
    }
}

// verifies: ARCH-020, ARCH-024
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_projects_archived_identity_without_substitution() {
    use xmtp_db::group::ConversationType;
    tester!(alix, disable_workers);
    let present = |creator: &str| {
        Some(ImmutableMetadataSave {
            creator_inbox_id: creator.into(),
        })
    };
    // (metadata, adder) -> (expected creator, expected adder)
    let cases = [
        (None, "archived-adder", "", "archived-adder"),
        (present(""), "archived-adder", "", "archived-adder"),
        (present("archived-creator"), "", "archived-creator", ""),
        (present(""), "", "", ""),
    ];
    for (index, (metadata, adder, creator, expected_adder)) in cases.into_iter().enumerate() {
        let mut save = saved_group(index as u8 + 10, false);
        save.metadata = metadata;
        save.added_by_inbox_id = adder.into();
        apply(&alix.context, vec![group_element(save.clone())]).await?;
        let group = alix.group(&GroupId::try_from(save.id.as_slice())?)?;

        let identity = group.metadata().await?;
        assert_eq!(identity.creator_inbox_id, creator);
        assert_ne!(identity.creator_inbox_id, alix.inbox_id());
        assert_eq!(identity.conversation_type, ConversationType::Group);
        assert_eq!(group.added_by_inbox_id()?, expected_adder);
        assert_ne!(group.added_by_inbox_id()?, alix.inbox_id());

        let mutable = group.mutable_metadata()?;
        assert_eq!(
            mutable.attributes.get("unknown-key").map(String::as_str),
            Some("retained value")
        );
        assert_eq!(mutable.admin_list, ["historical-admin"; 2]);
        assert_eq!(mutable.super_admin_list, ["historical-super-admin"]);
        assert_eq!(group.group_name()?, "historical name");
        assert_eq!(group.group_description()?, "");
        assert_eq!(group.admin_list()?, ["historical-admin"; 2]);
        assert_eq!(group.super_admin_list()?, ["historical-super-admin"]);
        assert!(group.is_super_admin("historical-super-admin".into())?);
        assert!(!group.is_super_admin(alix.inbox_id().to_string())?);
        assert!(group.conversation_message_disappearing_settings().is_err());

        let snapshot = group.state_snapshot()?;
        assert!(!snapshot.is_active);
        let snapshot = snapshot.group?;
        assert_eq!(snapshot.name, "historical name");
        assert_eq!(snapshot.admins, ["historical-admin"; 2]);
        assert_eq!(snapshot.super_admins, ["historical-super-admin"]);
        assert_eq!(snapshot.membership_state, GroupMembershipState::Restored);
    }

    // A foreign DM keeps its archived pair with an unknown creator.
    let mut dm = saved_group(20, true);
    dm.metadata = None;
    dm.message_disappear_in_ns = Some(200);
    apply(&alix.context, vec![group_element(dm.clone())]).await?;
    let group = alix.group(&GroupId::try_from(dm.id.as_slice())?)?;
    let identity = group.metadata().await?;
    assert_eq!(identity.creator_inbox_id, "");
    assert_eq!(identity.conversation_type, ConversationType::Dm);
    assert_eq!(identity.dm_members.map(|pair| pair.to_string()), dm.dm_id);
    assert_eq!(
        group.conversation_message_disappearing_settings()?,
        xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings::new(100, 200)
    );
}

// verifies: ARCH-020, ARCH-025
#[xmtp_common::test(unwrap_try = true)]
async fn restored_history_reexport_preserves_metadata_presence() {
    tester!(alix, disable_workers);
    let mut absent = saved_group(30, false);
    absent.metadata = None;
    let mut empty = saved_group(31, true);
    empty.metadata = Some(ImmutableMetadataSave {
        creator_inbox_id: String::new(),
    });
    let mut later = saved_group(32, false);
    later.last_message_ns = Some(50);
    for save in [&absent, &empty, &later] {
        apply(&alix.context, vec![group_element(save.clone())]).await?;
    }
    // A later message raises only the activity the re-export carries.
    stored_message(
        GroupId::try_from(later.id.as_slice())?,
        "a".repeat(64),
        0x60,
    )
    .store(&alix.db())?;
    let restored = GroupMembershipStateSave::from(GroupMembershipState::Restored) as i32;

    let exported = exported_groups(alix.db()).await?;
    assert_eq!(
        exported[&absent.id],
        GroupSave {
            membership_state: restored,
            ..absent.clone()
        }
    );
    assert_eq!(exported[&absent.id].metadata, None);
    assert_eq!(
        exported[&empty.id],
        GroupSave {
            membership_state: restored,
            ..empty.clone()
        }
    );
    assert_eq!(
        exported[&empty.id].metadata,
        Some(ImmutableMetadataSave {
            creator_inbox_id: String::new()
        })
    );
    assert_eq!(
        exported[&later.id],
        GroupSave {
            membership_state: restored,
            last_message_ns: Some(250),
            ..later.clone()
        }
    );
    for save in exported.values() {
        assert_ne!(
            save.metadata.as_ref().map(|m| m.creator_inbox_id.as_str()),
            Some(alix.inbox_id())
        );
    }
}

// verifies: ARCH-025, JOIN-080
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
    assert_eq!(handle.metadata().await?.creator_inbox_id, "");
    assert_eq!(handle.added_by_inbox_id()?, "archived-adder");
    assert_eq!(exported_groups(bo.db()).await?[&save.id].metadata, None);

    bo.sync_welcomes().await?;
    assert_ne!(
        bo.db().find_group(&dm.group_id)??.membership_state,
        GroupMembershipState::Restored
    );
    assert!(bo.db().restored_group_metadata(&dm.group_id)?.is_none());
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

// verifies: ARCH-015, ARCH-020
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

    // History is visible as history.
    assert_eq!(group.metadata().await?.creator_inbox_id, me);
    assert!(group.is_admin(me.clone())?);
    assert!(group.is_super_admin(me.clone())?);
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
