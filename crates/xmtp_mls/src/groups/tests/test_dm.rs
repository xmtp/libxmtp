use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::prelude::*;

use crate::context::XmtpSharedContext;
use crate::tester;
use crate::utils::TestMlsGroup;

fn stored_sender_message(
    group_id: xmtp_proto::types::GroupId,
    sender_inbox_id: String,
    marker: u8,
) -> xmtp_db::group_message::StoredGroupMessage {
    use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage};

    StoredGroupMessage {
        id: vec![marker; 32],
        group_id,
        decrypted_message_bytes: b"archived message".to_vec(),
        sent_at_ns: xmtp_common::time::now_ns(),
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

// verifies: DMS-015, DMS-018
#[cfg(not(target_arch = "wasm32"))]
#[rstest::rstest]
#[case::after_enumeration(false)]
#[case::after_first_row(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn stored_dm_startup_excludes_competing_writers(
    #[case] after_first_row: bool,
    #[values(false, true)] offline: bool,
) {
    use diesel::{Connection, connection::SimpleConnection};
    use std::{cell::RefCell, rc::Rc};
    use xmtp_db::group::GroupMembershipState;
    use xmtp_db::{
        ConnectionExt, EncryptedMessageStore, StorageError, StorageOption, TransactionOutcome,
        TransactionalKeyStore, XmtpMlsStorageProvider,
    };
    use xmtp_proto::types::GroupId;

    tester!(alix, persistent_db, disable_workers);
    let peer = hex::encode([0x42; 32]);
    let outside = hex::encode([0x43; 32]);
    let dm = TestMlsGroup::create_dm_and_insert(
        &alix.context,
        GroupMembershipState::Allowed,
        peer.clone(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
    )
    .unwrap();
    let mut placeholder = alix
        .db()
        .find_group(&dm.group_id)
        .unwrap()
        .expect("stored DM");
    placeholder.id = GroupId::ONE;
    placeholder.membership_state = GroupMembershipState::Restored;
    placeholder.store(&alix.db()).unwrap();
    let StorageOption::Persistent(path) = alix.context.store().opts() else {
        panic!("the competing writer needs a persistent database");
    };
    let other_store = EncryptedMessageStore::new(
        xmtp_db::database::NativeDb::builder()
            .persistent(path.clone())
            .key([0u8; 32])
            .single_connection()
            .build()
            .unwrap(),
    )
    .unwrap();
    let other_db = Rc::new(other_store.db());
    other_db
        .raw_query(|conn| {
            // This probe expects a lock error; disable only its connection's panic hook.
            conn.set_instrumentation(|_: diesel::connection::InstrumentationEvent<'_>| {});
            conn.batch_execute("PRAGMA busy_timeout = 0")
        })
        .unwrap();

    // This connection is opened before the scan and kept through both probes.
    let writer = xmtp_db::sql_key_store::SqlKeyStore::new(other_store.conn());
    let checkpoints = Rc::new(RefCell::new(Vec::new()));
    let observed = checkpoints.clone();
    let attempt = Rc::new(RefCell::new(None));
    let recorded = attempt.clone();
    let poison_sender = outside.clone();
    placeholder.id = GroupId::from([0x72; 16]);
    let inserted = placeholder.clone();
    let hook = crate::builder::dm_scan_test_hook::install(move |validated| {
        observed.borrow_mut().push(validated);
        if recorded.borrow().is_some() || validated.is_some() != after_first_row {
            return;
        }
        let target = validated.unwrap_or(inserted.id);
        let result = writer.transaction(|conn| {
            let storage = conn.key_store();
            let db = storage.db();
            if validated.is_none() {
                inserted.store(&db)?;
            }
            stored_sender_message(target, poison_sender.clone(), 0x73).store(&db)?;
            Ok::<_, StorageError>(TransactionOutcome::Continue(()))
        });
        match result {
            Ok(TransactionOutcome::Continue(())) => {
                *recorded.borrow_mut() = Some((target, true));
            }
            Ok(TransactionOutcome::Rollback) => panic!("the competing writer must not roll back"),
            Err(error) => {
                assert!(
                    matches!(error, StorageError::DieselResult(
                        diesel::result::Error::DatabaseError(_, ref info)
                    ) if info.message() == "database is locked"),
                    "the competing writer must fail with SQLite busy: {error:?}"
                );
                *recorded.borrow_mut() = Some((target, false));
            }
        }
    });
    let builder = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .with_allow_offline(Some(offline));
    let reopened = if offline {
        builder.build_offline()
    } else {
        builder.build().await
    };
    drop(hook);
    assert!(
        reopened.is_ok(),
        "valid state must open: {:?}",
        reopened.err()
    );
    {
        let checkpoints = checkpoints.borrow();
        assert_eq!(
            checkpoints.len(),
            3,
            "enumeration and both rows must be checked"
        );
        assert_eq!(checkpoints[0], None);
    }
    let (target, acquired) = attempt.borrow().expect("the writer probe must run");
    let poison_stored = alix
        .db()
        .has_sender_outside_pair(&target, [alix.inbox_id(), &peer])
        .unwrap();
    assert!(
        !acquired && !poison_stored,
        "startup missed a competing write: acquired={acquired}, poison_stored={poison_stored}"
    );
    assert!(alix.db().find_group(&placeholder.id).unwrap().is_none());

    // A successful write proves that the connection works and the scan released its lock.
    stored_sender_message(dm.group_id, alix.inbox_id().to_string(), 0x74)
        .store(other_db.as_ref())
        .unwrap();
    assert!(
        alix.db()
            .get_group_message(vec![0x74; 32])
            .unwrap()
            .is_some()
    );

    // A contradiction committed before the next scan must produce the exact error.
    placeholder.store(other_db.as_ref()).unwrap();
    stored_sender_message(placeholder.id, outside, 0x75)
        .store(other_db.as_ref())
        .unwrap();
    let builder = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .with_allow_offline(Some(offline));
    let rejected = if offline {
        builder.build_offline()
    } else {
        builder.build().await
    };
    assert!(matches!(
        rejected,
        Err(crate::builder::ClientBuilderError::GroupError(error))
            if matches!(
                *error,
                crate::groups::GroupError::MetadataPermissionsError(
                    crate::groups::MetadataPermissionsError::DmValidation(
                        crate::groups::DmValidationError::StoredMessageSenderOutsidePair
                    )
                )
            )
    ));
}

// verifies: DMS-018
#[xmtp_common::test(unwrap_try = true)]
async fn stored_dm_with_outside_membership_fails_client_open() {
    use openmls::prelude::{CredentialWithKey, MlsGroup as OpenMlsGroup};
    use xmtp_db::{Store, group::StoredGroup};
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId,
            creation::{InitialGroupKind, initial_dictionary},
            migration::{decode_group_membership_dict, encode_group_membership_dict},
        },
        group::GroupMetadataOptions,
        group_metadata::DmMembers,
        inbox_id::InboxId,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    tester!(alix, disable_workers);
    let peer = hex::encode([0x42; 32]);
    let outside = hex::encode([0x43; 32]);
    let context = &alix.context;
    let mut dictionary = initial_dictionary(
        InitialGroupKind::Dm {
            target_inbox_id: &peer,
        },
        &crate::groups::group_permissions::PolicySet::new_dm().to_proto()?,
        &GroupMetadataOptions::default(),
        alix.inbox_id(),
        None,
    )?;
    let mut entries = decode_group_membership_dict(
        dictionary
            .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
            .unwrap(),
    )?;
    entries.insert(
        InboxId::from_hex(&outside)?,
        GroupMembershipEntry {
            version: Some(Version::V1(V1 {
                sequence_id: 0,
                failed_installations: vec![],
            })),
        },
    );
    dictionary.insert(
        ComponentId::GROUP_MEMBERSHIP.as_u16(),
        encode_group_membership_dict(&entries)?,
    );
    let config = crate::groups::build_group_config(dictionary)?;
    let identity = context.identity();
    let mls_group = OpenMlsGroup::new(
        &context.mls_provider(),
        &identity.installation_keys,
        &config,
        CredentialWithKey {
            credential: identity.credential(),
            signature_key: identity.installation_keys.public_slice().into(),
        },
    )?;
    let group_id: xmtp_proto::types::GroupId = mls_group.group_id().try_into()?;
    StoredGroup::builder()
        .id(group_id)
        .created_at_ns(xmtp_common::time::now_ns())
        .membership_state(xmtp_db::group::GroupMembershipState::Allowed)
        .added_by_inbox_id(alix.inbox_id().to_string())
        .dm_id(Some(
            DmMembers {
                member_one_inbox_id: alix.inbox_id().to_string(),
                member_two_inbox_id: peer,
            }
            .to_string(),
        ))
        .build()?
        .store(&alix.db())?;

    let reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await;
    let Err(crate::builder::ClientBuilderError::GroupError(error)) = reopened else {
        panic!("stored DM with an outside member must fail client open");
    };
    assert!(matches!(
        *error,
        crate::groups::GroupError::MetadataPermissionsError(
            crate::groups::MetadataPermissionsError::DmValidation(
                crate::groups::DmValidationError::MemberOutsidePair
            )
        )
    ));
}

// verifies: DMS-018
#[xmtp_common::test(unwrap_try = true)]
async fn stored_dm_id_mismatch_fails_client_open() {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::ConnectionExt;

    tester!(alix, disable_workers);
    let dm = TestMlsGroup::create_dm_and_insert(
        &alix.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        hex::encode([0x42; 32]),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
    )?;
    alix.db().raw_query(|conn| {
        diesel::update(xmtp_db::schema::groups::table.find(dm.group_id))
            .set(xmtp_db::schema::groups::dm_id.eq(Some("dm:wrong".to_string())))
            .execute(conn)
    })?;

    let reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await;
    let Err(crate::builder::ClientBuilderError::GroupError(error)) = reopened else {
        panic!("stored DM with a wrong id must fail client open");
    };
    assert!(matches!(
        *error,
        crate::groups::GroupError::MetadataPermissionsError(
            crate::groups::MetadataPermissionsError::DmValidation(
                crate::groups::DmValidationError::StoredDmIdMismatch
            )
        )
    ));
}

// verifies: DMS-018
#[xmtp_common::test(unwrap_try = true)]
async fn stored_dm_with_outside_message_sender_fails_client_open() {
    use diesel::{QueryDsl, RunQueryDsl};
    use xmtp_db::{
        ConnectionExt, Store,
        group::GroupMembershipState,
        group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    };

    tester!(alix, disable_workers);
    let dm = TestMlsGroup::create_dm_and_insert(
        &alix.context,
        GroupMembershipState::Allowed,
        hex::encode([0x42; 32]),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
    )?;
    let mut message = StoredGroupMessage {
        id: vec![0x51; 32],
        group_id: dm.group_id,
        decrypted_message_bytes: b"pair message".to_vec(),
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: alix.context.installation_id().to_vec(),
        sender_inbox_id: alix.inbox_id().to_string(),
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
        idempotency_key: "pair-message".to_string(),
    };
    let db = alix.db();
    message.store(&db)?;
    let mut other_group = db.find_group(&dm.group_id)?.expect("stored DM");
    other_group.id = xmtp_proto::types::GroupId::ONE;
    other_group.membership_state = GroupMembershipState::Restored;
    other_group.store(&db)?;
    stored_sender_message(other_group.id, hex::encode([0x43; 32]), 0x55).store(&db)?;
    assert!(
        !db.has_sender_outside_pair(&dm.group_id, [alix.inbox_id(), &hex::encode([0x42; 32])],)?
    );
    let reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await;
    assert!(matches!(
        reopened,
        Err(crate::builder::ClientBuilderError::GroupError(error))
            if matches!(
                *error,
                crate::groups::GroupError::MetadataPermissionsError(
                    crate::groups::MetadataPermissionsError::DmValidation(
                        crate::groups::DmValidationError::StoredMessageSenderOutsidePair
                    )
                )
            )
    ));
    db.delete_message_by_id(vec![0x55; 32])?;
    db.raw_query(|conn| {
        diesel::delete(xmtp_db::schema::groups::table.find(other_group.id)).execute(conn)
    })?;
    // Evidence for the removed sibling remains, but it does not taint this
    // physical group.
    assert!(
        crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .build()
            .await
            .is_ok()
    );

    message.id = vec![0x52; 32];
    message.sender_inbox_id = hex::encode([0x43; 32]);
    message.idempotency_key = "outside-message".to_string();
    message.store(&db)?;
    assert!(
        db.has_sender_outside_pair(&dm.group_id, [alix.inbox_id(), &hex::encode([0x42; 32])],)?
    );
    let reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await;
    let Err(crate::builder::ClientBuilderError::GroupError(error)) = reopened else {
        panic!("stored DM with an outside message sender must fail client open");
    };
    assert!(matches!(
        *error,
        crate::groups::GroupError::MetadataPermissionsError(
            crate::groups::MetadataPermissionsError::DmValidation(
                crate::groups::DmValidationError::StoredMessageSenderOutsidePair
            )
        )
    ));

    // The compact sender record survives message deletion by design.
    db.delete_message_by_id(message.id)?;
    assert!(
        db.has_sender_outside_pair(&dm.group_id, [alix.inbox_id(), &hex::encode([0x42; 32])],)?
    );
}

// verifies: DMS-003, JOIN-080, JOIN-044
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_placeholder_activates_only_with_pair_senders() {
    use xmtp_db::{
        Store,
        group::{ConversationType, GroupMembershipState, QueryGroup, StoredGroup},
    };
    use xmtp_mls_common::group_metadata::DmMembers;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let db = bo.db();
    assert!(db.find_group(&dm.group_id)?.is_none());
    StoredGroup::builder()
        .id(dm.group_id)
        .created_at_ns(xmtp_common::time::now_ns())
        .membership_state(GroupMembershipState::Restored)
        .added_by_inbox_id(alix.inbox_id().to_string())
        .conversation_type(ConversationType::Dm)
        .dm_id(Some(
            DmMembers {
                member_one_inbox_id: alix.inbox_id().to_string(),
                member_two_inbox_id: bo.inbox_id().to_string(),
            }
            .to_string(),
        ))
        .build()?
        .store(&db)?;
    let archived = stored_sender_message(dm.group_id, bo.inbox_id().to_string(), 0x54);
    archived.store(&db)?;
    assert!(
        openmls::group::MlsGroup::load(bo.context.mls_storage(), &dm.group_id.to_openmls())?
            .is_none()
    );

    bo.sync_welcomes().await?;
    let stored = db
        .find_group(&dm.group_id)?
        .expect("Welcome must keep the DM row");
    assert_ne!(stored.membership_state, GroupMembershipState::Restored);
    let welcome_cursor = db.get_last_cursor(
        bo.context.installation_id(),
        xmtp_db::refresh_state::EntityKind::Welcome,
    )?;
    assert_eq!(stored.cursor(), Some(welcome_cursor));
    assert!(db.get_group_message(archived.id)?.is_some());
    assert!(bo.group(&dm.group_id)?.is_active()?);
}

// verifies: DMS-003, JOIN-080, JOIN-044
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_backup_stub_activates_with_pair_history() {
    use xmtp_db::{
        Store,
        group::{GroupMembershipState, QueryGroup},
    };

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    TestMlsGroup::create_dm_and_insert(
        &bo.context,
        GroupMembershipState::Restored,
        alix.inbox_id().to_string(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        Some(dm.group_id.as_ref()),
    )?;
    let db = bo.db();
    assert_eq!(
        db.find_group(&dm.group_id)?
            .expect("backup stub")
            .membership_state,
        GroupMembershipState::Restored
    );
    assert!(
        openmls::group::MlsGroup::load(bo.context.mls_storage(), &dm.group_id.to_openmls())?
            .is_some()
    );
    let archived = stored_sender_message(dm.group_id, bo.inbox_id().to_string(), 0x56);
    archived.store(&db)?;

    bo.sync_welcomes().await?;
    let stored = db.find_group(&dm.group_id)?.expect("activated DM");
    assert_ne!(stored.membership_state, GroupMembershipState::Restored);
    let welcome_cursor = db.get_last_cursor(
        bo.context.installation_id(),
        xmtp_db::refresh_state::EntityKind::Welcome,
    )?;
    assert_eq!(stored.cursor(), Some(welcome_cursor));
    assert!(db.get_group_message(archived.id)?.is_some());
    assert!(bo.group(&dm.group_id)?.is_active()?);
}

// verifies: DMS-003, DMS-015, JOIN-080
#[xmtp_common::test(unwrap_try = true)]
async fn restored_foreign_pair_rejects_different_welcome_pair() {
    use xmtp_db::group::{GroupMembershipState, QueryGroup};
    use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::DmMembers};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let foreign = hex::encode([0x43; 32]);
    let historical_pair = DmMembers {
        member_one_inbox_id: alix.inbox_id().to_string(),
        member_two_inbox_id: foreign,
    };
    TestMlsGroup::create_restored_dm_and_insert(
        &bo.context,
        historical_pair.clone(),
        GroupMetadataOptions::default(),
        dm.group_id.as_ref(),
    )?;
    let before = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(
        before.dm_id.as_deref(),
        Some(historical_pair.to_string().as_str())
    );

    let _ = bo.sync_welcomes().await;
    let after = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(after.membership_state, GroupMembershipState::Restored);
    assert_eq!(after.dm_id, before.dm_id);
}

// verifies: DMS-015, JOIN-080
#[xmtp_common::test(unwrap_try = true)]
async fn restored_group_rejects_dm_welcome_kind_change() {
    use xmtp_db::group::{ConversationType, GroupMembershipState, QueryGroup};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    TestMlsGroup::insert(
        &bo.context,
        Some(dm.group_id.as_ref()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
        false,
    )?;
    let _ = bo.sync_welcomes().await;
    let stored = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(stored.membership_state, GroupMembershipState::Restored);
    assert_eq!(stored.conversation_type, ConversationType::Group);
}

// verifies: DMS-015
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_deleted_outside_sender_evidence_fails_client_open() {
    use xmtp_db::group::GroupMembershipState;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    TestMlsGroup::create_dm_and_insert(
        &bo.context,
        GroupMembershipState::Restored,
        alix.inbox_id().to_string(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        Some(dm.group_id.as_ref()),
    )?;
    let bad = stored_sender_message(dm.group_id, hex::encode([0x43; 32]), 0x70);
    bad.store(&bo.db())?;
    assert_eq!(bo.db().delete_message_by_id(&bad.id)?, 1);
    assert!(bo.db().get_group_message(&bad.id)?.is_none());
    assert!(
        bo.db()
            .has_sender_outside_pair(&dm.group_id, [alix.inbox_id(), bo.inbox_id()],)?
    );

    let reopened = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .with_disable_workers(true)
        .build()
        .await;
    let Err(crate::builder::ClientBuilderError::GroupError(error)) = reopened else {
        panic!("Restored DM with old outside-sender evidence must fail client open");
    };
    assert!(matches!(
        *error,
        crate::groups::GroupError::MetadataPermissionsError(
            crate::groups::MetadataPermissionsError::DmValidation(
                crate::groups::DmValidationError::StoredMessageSenderOutsidePair
            )
        )
    ));
}

// verifies: DMS-003, JOIN-080
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_placeholder_rejects_outside_sender_on_welcome() {
    use xmtp_db::{
        Store,
        group::{ConversationType, GroupMembershipState, QueryGroup, StoredGroup},
    };
    use xmtp_mls_common::group_metadata::DmMembers;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let db = bo.db();
    StoredGroup::builder()
        .id(dm.group_id)
        .created_at_ns(xmtp_common::time::now_ns())
        .membership_state(GroupMembershipState::Restored)
        .added_by_inbox_id(alix.inbox_id().to_string())
        .conversation_type(ConversationType::Dm)
        .dm_id(Some(
            DmMembers {
                member_one_inbox_id: alix.inbox_id().to_string(),
                member_two_inbox_id: bo.inbox_id().to_string(),
            }
            .to_string(),
        ))
        .build()?
        .store(&db)?;
    stored_sender_message(dm.group_id, hex::encode([0x43; 32]), 0x53).store(&db)?;
    assert!(
        openmls::group::MlsGroup::load(bo.context.mls_storage(), &dm.group_id.to_openmls())?
            .is_none()
    );

    bo.sync_welcomes().await?;
    let stored = db
        .find_group(&dm.group_id)?
        .expect("rejected Welcome must keep the placeholder");
    assert_eq!(stored.membership_state, GroupMembershipState::Restored);
    let topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: bo.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let rejected = db
        .read_last_rejection(&topic)?
        .expect("Welcome must be rejected");
    assert_eq!(rejected.code, "invalid_welcome");
    assert!(db.pending_envelope(&topic, rejected.sequence_id)?.is_none());
}

// verifies: DMS-003
#[xmtp_common::test(unwrap_try = true)]
async fn stored_dm_states_and_restored_placeholder_allow_client_open() {
    use xmtp_db::group::GroupMembershipState;
    use xmtp_db::{Store, group::QueryGroup};

    tester!(alix, disable_workers);
    let dm = TestMlsGroup::create_dm_and_insert(
        &alix.context,
        GroupMembershipState::Allowed,
        hex::encode([0x42; 32]),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
    )?;
    let db = alix.db();
    let mut placeholder = db.find_group(&dm.group_id)?.unwrap();
    placeholder.id = xmtp_proto::types::GroupId::ONE;
    placeholder.membership_state = GroupMembershipState::Restored;
    placeholder.store(&db)?;
    for state in [
        GroupMembershipState::Pending,
        GroupMembershipState::Allowed,
        GroupMembershipState::PendingRemove,
        GroupMembershipState::Rejected,
    ] {
        db.update_group_membership(dm.group_id, state)?;
        let reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .build()
            .await;
        assert!(
            reopened.is_ok(),
            "valid DM in {state:?} state and placeholder must reopen: {:?}",
            reopened.err()
        );
    }
}

/// Test case: If two users are talking in a DM, and one user
/// creates a new installation and creates a new DM before being
/// welcomed into the old DM, that new DM group should be consented.
#[xmtp_common::test(unwrap_try = true)]
async fn auto_consent_dms_for_new_installations() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Bo creates a new installation and immediately creates a new DM with alix
    let bo2_dm = bo2.find_or_create_dm(alix.inbox_id(), None).await?;

    // Alix pulls down the new DM from bo
    alix.sync_welcomes().await?;

    // That DM should be already consented, since alix consented with bo in another DM
    let consent = alix
        .get_consent_state(ConsentType::ConversationId, hex::encode(bo2_dm.group_id))
        .await?;
    assert_eq!(consent, ConsentState::Allowed);
}

/// Test case: If a second installation syncs the consent state for a DM
/// before processing the welcome, the welcome should succeed rather than
/// aborting on a unique constraint error.
#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_welcome_with_preexisting_consent() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    let (a_group, _) = alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Mock device sync - the consent record is processed on Bo2 before
    // the welcome is processed.
    let cr = StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(a_group.group_id),
    );
    bo2.context.db().insert_newer_consent_record(cr)?;
    // Now bo2 processes the welcome
    bo1.find_or_create_dm(alix.inbox_id(), None)
        .await?
        .update_installations()
        .await?;
    bo2.sync_welcomes().await?;

    // The welcome should succeed
    assert_eq!(
        bo2.find_or_create_dm(alix.inbox_id(), None).await?.group_id,
        a_group.group_id
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_group_update_dedupes() {
    tester!(alix);
    tester!(bo);

    let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

    let updates = || {
        dm.find_messages(&MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        })?
    };
    assert_eq!(updates().len(), 1);

    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // The same event in a row will be deduped
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // Different time means different update, will not be deduped.
    dm.update_conversation_message_disappear_from_ns(2).await?;
    assert_eq!(updates().len(), 3);

    // Back to 1, will not be deduped because we set it to 2 and back.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);

    // Continue to dedupe because the field did not change.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);
}

fn dictionary_native_dm(
    context: &impl XmtpSharedContext,
    added_by_inbox: &str,
    allow_add_member: bool,
) -> openmls::group::MlsGroup {
    use crate::groups::group_permissions::{MembershipPolicies, PolicySet};
    use xmtp_mls_common::app_data::creation::synthesize_registry_from_policy_set;

    // Build the dictionary directly. DM bootstrap synthesis is a separate task.
    let mut policies = PolicySet::new_dm();
    if allow_add_member {
        policies.add_member_policy = MembershipPolicies::allow();
    }
    let policy_proto = policies.to_proto().unwrap();
    let registry = synthesize_registry_from_policy_set(&policy_proto).unwrap();
    dictionary_native_dm_with_registry(context, added_by_inbox, registry)
}

fn dictionary_native_dm_with_registry(
    context: &impl XmtpSharedContext,
    added_by_inbox: &str,
    registry: xmtp_mls_common::app_data::component_registry::ComponentRegistry,
) -> openmls::group::MlsGroup {
    use openmls::{
        extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
        prelude::{Capabilities, CredentialWithKey, ExtensionType, MlsGroupCreateConfig},
    };
    use tls_codec::Serialize;
    use xmtp_mls_common::{
        app_data::{component_id::ComponentId, migration::encode_group_membership_dict},
        inbox_id::InboxId,
        tls_set::TlsSet,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    let creator = InboxId::from_hex(added_by_inbox).unwrap();
    let recipient = InboxId::from_hex(context.inbox_id()).unwrap();
    let membership = std::collections::BTreeMap::from([(
        recipient,
        GroupMembershipEntry {
            version: Some(Version::V1(V1 {
                sequence_id: 0,
                failed_installations: vec![],
            })),
        },
    )]);
    let mut dictionary = AppDataDictionary::new();
    for (id, bytes) in [
        (
            ComponentId::COMPONENT_REGISTRY,
            registry.to_bytes().unwrap(),
        ),
        (
            ComponentId::CONVERSATION_TYPE,
            (xmtp_proto::types::ConversationType::Dm as i32)
                .to_be_bytes()
                .to_vec(),
        ),
        (
            ComponentId::CREATOR_INBOX_ID,
            creator.tls_serialize_detached().unwrap(),
        ),
        (
            ComponentId::DM_MEMBERS,
            TlsSet::from_keys([creator, recipient])
                .tls_serialize_detached()
                .unwrap(),
        ),
        (
            ComponentId::GROUP_MEMBERSHIP,
            encode_group_membership_dict(&membership).unwrap(),
        ),
        (
            ComponentId::ADMIN_LIST,
            TlsSet::<InboxId>::new().tls_serialize_detached().unwrap(),
        ),
        (
            ComponentId::SUPER_ADMIN_LIST,
            TlsSet::<InboxId>::new().tls_serialize_detached().unwrap(),
        ),
    ] {
        assert!(dictionary.insert(id.as_u16(), bytes).is_none());
    }
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap();
    let config = MlsGroupCreateConfig::builder()
        .with_group_context_extensions(extensions)
        .capabilities(Capabilities::new(
            None,
            None,
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        ))
        .ciphersuite(xmtp_cryptography::configuration::CIPHERSUITE)
        .build();
    let identity = context.identity();
    openmls::group::MlsGroup::new(
        &context.mls_provider(),
        &identity.installation_keys,
        &config,
        CredentialWithKey {
            credential: identity.credential(),
            signature_key: identity.installation_keys.public_slice().into(),
        },
    )
    .unwrap()
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_dm_accepts_valid_permissions() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    bo.sync_welcomes().await?;
    let received = bo.group(&dm.group_id)?;
    assert!(received.mutable_metadata()?.admin_list.is_empty());
    assert!(received.mutable_metadata()?.super_admin_list.is_empty());
    assert_eq!(received.members().await?.len(), 2);
    dm.send_message(b"dictionary-native DM", Default::default())
        .await?;
    received.sync().await?;
    assert!(
        received
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| { message.decrypted_message_bytes == b"dictionary-native DM" })
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn canonical_dm_registry_matches_validation_expectation() {
    use crate::groups::group_permissions::PolicySet;
    use xmtp_mls_common::app_data::{
        component_registry::ComponentRegistry,
        migration::synthesize_canonical_subset_from_extensions,
    };
    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let legacy_dm = TestMlsGroup::create_test_dm_group(
        alix.context.clone(),
        added_by.clone(),
        None,
        None,
        None,
        None,
        None,
    )?;
    let entries = legacy_dm.load_mls_group_with_lock(alix.context.mls_storage(), |group| {
        Ok(
            synthesize_canonical_subset_from_extensions(group.extensions())
                .unwrap()
                .expected_registry,
        )
    })?;
    let mut registry = ComponentRegistry::new();
    for (id, metadata) in entries {
        registry.set(id, metadata)?;
    }
    let group = dictionary_native_dm_with_registry(&alix.context, &added_by, registry);
    let actual =
        crate::groups::group_permissions::policy_set_from_dictionary(group.extensions())?.policies;
    let expected = PolicySet::new_dm();
    assert_eq!(actual.add_member_policy, expected.add_member_policy);
    assert_eq!(actual.remove_member_policy, expected.remove_member_policy);
    assert_eq!(actual.add_admin_policy, expected.add_admin_policy);
    assert_eq!(actual.remove_admin_policy, expected.remove_admin_policy);
    crate::groups::validate_dm_group(&alix.context, &group, &added_by)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_dm_rejects_add_member_policy_alone() {
    use crate::groups::{DmValidationError, MetadataPermissionsError};

    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let group = dictionary_native_dm(&alix.context, &added_by, true);
    let result = crate::groups::validate_dm_group(&alix.context, &group, &added_by);
    assert!(
        matches!(
            result,
            Err(MetadataPermissionsError::DmValidation(
                DmValidationError::InvalidPermissions
            ))
        ),
        "unexpected validation result: {result:?}"
    );
}
