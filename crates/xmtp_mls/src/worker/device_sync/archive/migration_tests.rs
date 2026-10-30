//! The legacy exporter must feed the existing SDK importer.
use super::*;
use crate::tester;
use diesel::{connection::SimpleConnection, prelude::*};
use prost::Message as _;
use xmtp_db::{
    ConnectionExt,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord},
    group::{GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, StoredGroupMessage},
    schema::{group_messages, groups},
};
use xmtp_legacy_migration::{PrepareMigrationArchiveArgs, prepare_migration_archive};
use xmtp_proto::xmtp::message_contents::{ContentTypeId, EncodedContent};

const KEY: [u8; 32] = [7; 32];
const DM: &str = "a06859a4aebe75c970b8875698fbeddb";
/// The group of `appdata-context.bincode`, the context the AppData producer
/// stored.
const APPDATA_GROUP: &str = "f23017122a085cf60892ae13ae53412c";

// verifies: MIG-001, MIG-003, MIG-005, MIG-006, ARCH-007, ARCH-010, CONS-050
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_migration_imports_into_empty_and_populated_stores_and_retries() {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("legacy.db3");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xmtp_legacy_migration/fixtures/consent-states.db3"
        ),
        &source,
    )?;
    let mut legacy = SqliteConnection::establish(source.to_str().unwrap())?;
    diesel::sql_query("UPDATE group_messages SET delivery_status=1 WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(vec![8; 32])
        .execute(&mut legacy)?;
    diesel::sql_query("UPDATE group_messages SET delivery_status=3 WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(vec![10; 32])
        .execute(&mut legacy)?;
    // Keep the pinned legacy deletion authority and content-type rules.
    legacy.batch_execute("PRAGMA foreign_keys=OFF;")?;
    for id in 20..=26u8 {
        diesel::sql_query("INSERT INTO group_messages(id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, sender_inbox_id, delivery_status, content_type, authority_id, originator_id, sequence_id) SELECT ?, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, sender_inbox_id, 2, ?, authority_id, originator_id, sequence_id FROM group_messages WHERE id=?")
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .bind::<diesel::sql_types::Integer, _>(if id == 20 { 3 } else if id == 24 { 0 } else { 1 })
            .bind::<diesel::sql_types::Binary, _>(vec![1; 32])
            .execute(&mut legacy)?;
        diesel::sql_query("INSERT INTO message_deletions(id, group_id, deleted_message_id, deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) SELECT ?, ?, ?, ?, ?, 1700000000000000999")
            .bind::<diesel::sql_types::Binary, _>(vec![id + 100; 32])
            .bind::<diesel::sql_types::Binary, _>(if id == 23 { vec![0x66; 16] } else { hex::decode(DM)? })
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .bind::<diesel::sql_types::Text, _>(if id == 20 || id == 24 || id >= 25 { "01".repeat(32) } else { "02".repeat(32) })
            .bind::<diesel::sql_types::Bool, _>(id == 21 || id == 23)
            .execute(&mut legacy)?;
    }
    let non_deletable = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "example.test".into(),
            type_id: "custom".into(),
            version_major: 1,
            version_minor: 0,
        }),
        content: b"opaque non-deletable fixture".to_vec(),
        ..Default::default()
    }
    .encode_to_vec();
    diesel::sql_query("UPDATE group_messages SET decrypted_message_bytes=? WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(&non_deletable)
        .bind::<diesel::sql_types::Binary, _>(vec![24; 32])
        .execute(&mut legacy)?;
    // The pinned legacy classifier uses type_id only, including other
    // authorities and major versions. Current catalogue rules do not apply.
    for (id, authority, major) in [(25, "custom.example", 1), (26, "xmtp.org", 2)] {
        let content = EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: authority.into(),
                type_id: "text".into(),
                version_major: major,
                version_minor: 0,
            }),
            content: b"deleted legacy text".to_vec(),
            ..Default::default()
        }
        .encode_to_vec();
        diesel::sql_query("UPDATE group_messages SET decrypted_message_bytes=? WHERE id=?")
            .bind::<diesel::sql_types::Binary, _>(&content)
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .execute(&mut legacy)?;
    }
    // The AppData fixture's group: a group row like the other group's, and the
    // group context row the AppData producer stored, under its own group id.
    let appdata_group = hex::decode(APPDATA_GROUP)?;
    diesel::sql_query("INSERT INTO groups(id, created_at_ns, membership_state, installations_last_checked, added_by_inbox_id, conversation_type) SELECT ?, created_at_ns, membership_state, installations_last_checked, added_by_inbox_id, conversation_type FROM groups WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(&appdata_group)
        .bind::<diesel::sql_types::Binary, _>(vec![0x44; 16])
        .execute(&mut legacy)?;
    let context_key = [
        b"GroupContextGroupContext".as_slice(),
        &16u64.to_le_bytes(),
        &appdata_group,
        &[0, 1, 0, 1],
    ]
    .concat();
    diesel::sql_query(
        "INSERT INTO openmls_key_value(key_bytes, version, value_bytes) VALUES (?, 1, ?)",
    )
    .bind::<diesel::sql_types::Binary, _>(context_key)
    .bind::<diesel::sql_types::Binary, _>(
        include_bytes!("../../../../../xmtp_legacy_migration/fixtures/appdata-context.bincode")
            .as_slice(),
    )
    .execute(&mut legacy)?;
    drop(legacy);
    let output = directory.path().join("migration.xmtp");
    let report = prepare_migration_archive(PrepareMigrationArchiveArgs {
        database_path: source.to_str().unwrap().into(),
        database_key: None,
        archive_key: KEY.to_vec(),
        output_path: output.to_str().unwrap().into(),
    })
    .await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (3, 6, 3)
    );
    // The metadata fixtures: the DM from `metadata-seed.db3`, with the values
    // its producer recorded, and the AppData group.
    let dm_expected: serde_json::Value = serde_json::from_slice(&std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../xmtp_legacy_migration/fixtures/metadata-expected.json"
    ))?)?;
    let text = |value: &serde_json::Value| value.as_str().expect("fixture string").to_owned();
    let metadata_fixtures = [
        (
            DM,
            text(&dm_expected["creator_inbox_id"]),
            text(&dm_expected["attributes"]["group_name"]),
            text(&dm_expected["attributes"]["description"]),
        ),
        (
            APPDATA_GROUP,
            "01".repeat(32),
            "AppData Group".to_owned(),
            "good description".to_owned(),
        ),
    ];
    // The archive carries each legacy creator. The importer gives a restored
    // group no archived authority, so the creator is checked here.
    let mut archived_creators = std::collections::HashMap::new();
    let mut importer = ArchiveImporter::from_file(&output, &KEY).await?;
    while let Some(element) = importer.next().await {
        if let Some(Element::Group(save)) = element?.element {
            archived_creators.insert(
                hex::encode(&save.id),
                save.metadata.map(|metadata| metadata.creator_inbox_id),
            );
        }
    }
    for (id, creator, _, _) in &metadata_fixtures {
        assert_eq!(
            archived_creators.get(*id),
            Some(&Some(creator.clone())),
            "creator of {id}"
        );
    }
    for populated in [false, true] {
        tester!(destination, disable_workers);
        let original = if populated {
            let group = destination.create_group(None, None)?;
            let message = group
                .send_message(b"destination history", Default::default())
                .await?;
            Some((group.group_id, message))
        } else {
            None
        };
        for _ in 0..2 {
            let mut importer = ArchiveImporter::from_file(&output, &KEY).await?;
            insert_importer(&mut importer, &destination.context).await?;
            let restored: Vec<StoredGroup> = destination.db().raw_query(|conn| {
                groups::table
                    .filter(groups::membership_state.eq(GroupMembershipState::Restored as i32))
                    .load(conn)
            })?;
            assert_eq!(restored.len(), 3);
            for (prefix, state) in [
                ("02", ConsentState::Unknown),
                ("03", ConsentState::Allowed),
                ("04", ConsentState::Denied),
            ] {
                let consent = destination
                    .db()
                    .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(
                        &prefix.repeat(32),
                    ))?
                    .expect("imported consent record");
                assert_eq!(consent.state, state);
                assert_eq!(consent.consented_at_ns, 1700000000000000009);
            }
            let dm = restored
                .iter()
                .find(|g| hex::encode(g.id) == DM)
                .expect("original DM id");
            assert_eq!(
                dm.dm_id.as_deref(),
                Some(format!("dm:{}:{}", "01".repeat(32), "02".repeat(32)).as_str())
            );
            for (id, _, name, description) in &metadata_fixtures {
                let group = destination.group(&id.parse()?)?;
                assert_eq!(&group.group_name()?, name, "name of {id}");
                assert_eq!(
                    &group.group_description()?,
                    description,
                    "description of {id}"
                );
            }
            let messages: Vec<StoredGroupMessage> = destination.db().raw_query(|conn| {
                group_messages::table
                    .filter(group_messages::group_id.eq_any(restored.iter().map(|group| &group.id)))
                    .order(group_messages::id)
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })?;
            assert_eq!(
                messages
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<Vec<_>>(),
                [1, 8, 10, 22, 23, 24].map(|id| vec![id; 32])
            );
            for message in messages {
                assert_eq!(
                    message.delivery_status,
                    match message.id[0] {
                        1 | 22 | 23 | 24 => DeliveryStatus::Published,
                        8 => DeliveryStatus::Unpublished,
                        10 => DeliveryStatus::Failed,
                        id => panic!("unexpected migrated message: {id}"),
                    }
                );
                assert_eq!(
                    message.sent_at_ns,
                    if message.id == vec![10; 32] {
                        1500000000000000000
                    } else {
                        1700000000000000123
                    }
                );
                assert_eq!(
                    hex::encode(message.decrypted_message_bytes),
                    if message.id == vec![24; 32] {
                        hex::encode(&non_deletable)
                    } else {
                        "0a120a08786d74702e6f7267120474657874180122166d6967726174696f6e20666978747572652074657874".to_owned()
                    }
                );
            }
            if let Some((id, message)) = &original {
                let group = destination
                    .db()
                    .find_group(id)?
                    .expect("existing destination group");
                assert_ne!(group.membership_state, GroupMembershipState::Restored);
                let count: i64 = destination.db().raw_query(|conn| {
                    group_messages::table
                        .filter(group_messages::id.eq(message))
                        .count()
                        .get_result(conn)
                })?;
                assert_eq!(count, 1);
            }
        }
    }
}

// The real database from @xmtp/react-native-sdk 5.7.0 converts and imports
// into a client with its group metadata, messages, and consent.
// verifies: MIG-001
#[xmtp_common::test(unwrap_try = true)]
async fn react_native_5_7_0_database_imports_into_a_client() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../xmtp_legacy_migration/fixtures/react-native-5.7.0"
    );
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{fixture}/expected-records.json"))?)?;
    let text = |value: &serde_json::Value| value.as_str().expect("fixture string").to_owned();
    let directory = tempfile::tempdir()?;
    for suffix in ["", ".sqlcipher_salt"] {
        let name = format!("react-native-5.7.0.db3{suffix}");
        std::fs::copy(format!("{fixture}/{name}"), directory.path().join(name))?;
    }
    let output = directory.path().join("migration.xmtp");
    prepare_migration_archive(PrepareMigrationArchiveArgs {
        database_path: directory
            .path()
            .join("react-native-5.7.0.db3")
            .to_str()
            .unwrap()
            .into(),
        database_key: Some(hex::decode(text(&expected["databaseKeyHex"]))?),
        archive_key: KEY.to_vec(),
        output_path: output.to_str().unwrap().into(),
    })
    .await?;

    tester!(destination, disable_workers);
    let mut importer = ArchiveImporter::from_file(&output, &KEY).await?;
    insert_importer(&mut importer, &destination.context).await?;

    let [record] = expected["groups"].as_array().expect("groups").as_slice() else {
        panic!("the fixture has one group");
    };
    let group_id: xmtp_proto::types::GroupId = text(&record["id"]).parse()?;
    let group = destination.group(&group_id)?;
    assert_eq!(group.group_name()?, text(&record["name"]));
    assert_eq!(group.group_description()?, text(&record["description"]));
    let messages: Vec<StoredGroupMessage> = destination.db().raw_query(|conn| {
        group_messages::table
            .filter(group_messages::group_id.eq(&group_id))
            .order(group_messages::sent_at_ns)
            .select(StoredGroupMessage::as_select())
            .load(conn)
    })?;
    let mut expected_messages = expected["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|message| {
            (
                text(&message["id"]),
                text(&message["senderInboxId"]),
                text(&message["sentAtNs"]),
                text(&message["encodedContentHex"]),
            )
        })
        .collect::<Vec<_>>();
    expected_messages.sort_by(|a, b| a.2.cmp(&b.2));
    assert_eq!(
        messages
            .iter()
            .map(|message| (
                hex::encode(&message.id),
                message.sender_inbox_id.clone(),
                message.sent_at_ns.to_string(),
                hex::encode(&message.decrypted_message_bytes),
            ))
            .collect::<Vec<_>>(),
        expected_messages
    );
    for record in expected["consent"].as_array().expect("consent") {
        let kind = match record["entityType"].as_str() {
            Some("conversation_id") => ConsentType::ConversationId,
            Some("inbox_id") => ConsentType::InboxId,
            other => panic!("unexpected fixture entity type {other:?}"),
        };
        let state = match record["state"].as_str() {
            Some("allowed") => ConsentState::Allowed,
            Some("denied") => ConsentState::Denied,
            other => panic!("unexpected fixture state {other:?}"),
        };
        let key =
            xmtp_db::consent_record::ConsentKey::parse(kind, &text(&record["entity"])).unwrap();
        let consent = destination
            .db()
            .consent_row(&key)?
            .expect("imported consent record");
        assert_eq!(consent.state, state);
    }
}
