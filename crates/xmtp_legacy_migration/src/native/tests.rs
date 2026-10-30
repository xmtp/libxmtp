use super::*;
use crate::prepare_migration_archive;
use futures::TryStreamExt;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
use xmtp_archive::importer::ArchiveImporter;
use xmtp_proto::xmtp::device_sync::backup_element::Element;

const KEY: [u8; 32] = [7; 32];
const CONTENT_HEX: &str =
    "0a120a08786d74702e6f7267120474657874180122166d6967726174696f6e20666978747572652074657874";
const OWNER: &str = "0101010101010101010101010101010101010101010101010101010101010101";
const PEER: &str = "0202020202020202020202020202020202020202020202020202020202020202";
const GROUP: &str = "a06859a4aebe75c970b8875698fbeddb";

fn fixture(name: &str) -> (tempfile::TempDir, PrepareMigrationArchiveArgs) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.db3");
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    for suffix in SIDECARS {
        let from = sidecar(&original, suffix);
        if from.exists() {
            fs::copy(from, sidecar(&path, suffix)).unwrap();
        }
    }
    let args = PrepareMigrationArchiveArgs {
        database_path: path.to_str().unwrap().to_owned(),
        database_key: name.starts_with("encrypted").then(|| vec![0x11; 32]),
        archive_key: KEY.to_vec(),
        output_path: directory
            .path()
            .join("history.xmtp")
            .to_str()
            .unwrap()
            .to_owned(),
    };
    (directory, args)
}

fn source_bytes(args: &PrepareMigrationArchiveArgs) -> BTreeMap<&'static str, Vec<u8>> {
    SIDECARS
        .iter()
        .filter_map(
            |&suffix| match fs::read(sidecar(Path::new(&args.database_path), suffix)) {
                Ok(bytes) => Some((suffix, bytes)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => panic!("source read failed: {e}"),
            },
        )
        .collect()
}

async fn elements(path: &str) -> Vec<Element> {
    let importer = ArchiveImporter::from_file(path, &KEY).await.unwrap();
    assert_eq!(importer.metadata.elements.len(), 2);
    importer
        .map_ok(|e| e.element.unwrap())
        .try_collect()
        .await
        .unwrap()
}

fn edit(args: &PrepareMigrationArchiveArgs, sql: &str) {
    let mut conn = SqliteConnection::establish(&args.database_path).unwrap();
    conn.batch_execute("PRAGMA foreign_keys=OFF;").unwrap();
    conn.batch_execute(sql).unwrap();
}

fn entries(path: &Path) -> Vec<std::ffi::OsString> {
    let mut names = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[derive(Clone, Copy)]
enum ExpectedError {
    RecordRead,
    UnsupportedSchema,
}

async fn rejects_without_publishing(
    args: &PrepareMigrationArchiveArgs,
    directory: &Path,
    name: &str,
    expected: ExpectedError,
) -> Vec<String> {
    let before = source_bytes(args);
    let mut failures = vec![];
    for previous in [false, true] {
        if previous {
            fs::write(&args.output_path, b"completed archive").unwrap();
        }
        let before_entries = entries(directory);
        let result = prepare_migration_archive(args.clone()).await;
        let matches = matches!(
            (&result, expected),
            (
                Err(MigrationError::RecordRead(_)),
                ExpectedError::RecordRead
            ) | (
                Err(MigrationError::UnsupportedSchema),
                ExpectedError::UnsupportedSchema
            )
        );
        if !matches {
            failures.push(format!("{name}, previous={previous}: {result:?}"));
            if Path::new(&args.output_path).exists() {
                fs::remove_file(&args.output_path).unwrap();
            }
        } else {
            if previous {
                assert_eq!(fs::read(&args.output_path).unwrap(), b"completed archive");
            } else {
                assert!(!Path::new(&args.output_path).exists());
            }
            assert_eq!(entries(directory), before_entries, "private output remains");
        }
        assert_eq!(source_bytes(args), before);
    }
    failures
}

// verifies: MIG-001, MIG-002, MIG-003, MIG-005, MIG-006, ARCH-007, ARCH-008, ARCH-010, ARCH-026
#[xmtp_common::test(unwrap_try = true)]
async fn stable_history_metadata_and_exclusions() {
    for name in ["stable.db3", "mobile-4.10.db3"] {
        let (_directory, args) = fixture(name);
        let before = source_bytes(&args);
        let report = prepare_migration_archive(args.clone()).await?;
        assert_eq!(
            (
                report.group_count,
                report.message_count,
                report.consent_count
            ),
            (2, 3, 1)
        );
        assert_eq!(report.archive_path, args.output_path);
        assert_eq!(source_bytes(&args), before);
        let records = elements(&report.archive_path).await;
        assert_eq!(records.len(), 6);
        let Element::Group(unknown) = &records[0] else {
            panic!("group must precede messages")
        };
        assert_eq!(unknown.id, vec![0x44; 16]);
        assert!(unknown.metadata.is_none());
        assert!(unknown.mutable_metadata.is_none());
        let Element::Group(group) = &records[1] else {
            panic!("group must precede messages")
        };
        assert_eq!(hex::encode(&group.id), GROUP);
        assert_eq!(group.created_at_ns, 1600000000000000001);
        assert_eq!(group.metadata.as_ref().unwrap().creator_inbox_id, OWNER);
        let mutable = group.mutable_metadata.as_ref().unwrap();
        assert_eq!(mutable.attributes["group_name"], "Migration DM");
        assert_eq!(mutable.attributes["description"], "Legacy fixture");
        assert_eq!(mutable.attributes["app_data"], "fixture-app-data");
        assert_eq!(mutable.admin_list, [PEER]);
        assert_eq!(mutable.super_admin_list, [OWNER]);
        assert_eq!(group.message_disappear_from_ns, Some(1700000000000000000));
        assert_eq!(group.message_disappear_in_ns, Some(60000000000));
        for (record, id) in records[2..5].iter().zip([1, 8, 10]) {
            let Element::GroupMessage(message) = record else {
                panic!("missing history")
            };
            assert_eq!(message.id, vec![id; 32]);
            assert_eq!(hex::encode(&message.decrypted_message_bytes), CONTENT_HEX);
            assert_eq!(
                message.sent_at_ns,
                if id == 10 {
                    1500000000000000000
                } else {
                    1700000000000000123
                }
            );
            assert_eq!(message.authority_id, "xmtp.org");
            assert_eq!(message.content_type, "text");
            assert_eq!(message.version_major, 1);
            assert_eq!(message.delivery_status, 2);
            assert_eq!(message.expires_at_ns, None);
            assert_eq!(message.expiry_known, None);
        }
        let Element::Consent(consent) = &records[5] else {
            panic!("missing consent")
        };
        assert_eq!(consent.entity, PEER);
        assert_eq!(consent.state, 2);
        assert_eq!(consent.consented_at_ns, 1700000000000000009);
    }
}

// The closed SQLCipher database that the libxmtp source embedded in
// @xmtp/react-native-sdk 5.7.0 wrote. Its producer read the expected records
// through the old public API (fixtures/react-native-5.7.0/README.md).
// verifies: MIG-001, MIG-002, MIG-003
#[xmtp_common::test(unwrap_try = true)]
async fn react_native_5_7_0_database_exports_its_independent_records() {
    use sha2::{Digest, Sha256};
    use xmtp_proto::xmtp::device_sync::consent_backup::{ConsentStateSave, ConsentTypeSave};

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/react-native-5.7.0");
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_dir.join("expected-records.json"))?)?;
    let text = |value: &serde_json::Value| value.as_str().expect("fixture string").to_owned();
    let (_directory, mut args) = fixture("react-native-5.7.0/react-native-5.7.0.db3");
    args.database_key = Some(hex::decode(text(&expected["databaseKeyHex"]))?);
    let before = source_bytes(&args);
    assert_eq!(
        before.keys().copied().collect::<Vec<_>>(),
        ["", ".sqlcipher_salt"],
        "the closed source has no WAL, SHM, or journal"
    );
    // The committed bytes are the ones the provenance record describes.
    let inspection: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_dir.join("source-inspection.json"))?)?;
    for (suffix, bytes) in &before {
        let recorded = &inspection["sourceHashesBefore"][format!("react-native-5.7.0.db3{suffix}")];
        assert_eq!(
            (hex::encode(Sha256::digest(bytes)), bytes.len() as u64),
            (
                text(&recorded["sha256"]),
                recorded["bytes"].as_u64().expect("byte count")
            ),
            "source-inspection.json records react-native-5.7.0.db3{suffix}"
        );
    }

    let report = prepare_migration_archive(args.clone()).await?;
    let counts = &expected["counts"];
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (
            counts["groups"].as_u64().expect("group count"),
            counts["messages"].as_u64().expect("message count"),
            counts["consent"].as_u64().expect("consent count"),
        )
    );
    assert_eq!(report.archive_path, args.output_path);
    assert_eq!(source_bytes(&args), before);

    let records = elements(&report.archive_path).await;
    let mut groups = vec![];
    let mut messages = vec![];
    let mut consent = vec![];
    for record in records {
        match record {
            Element::Group(group) => {
                let mutable = group.mutable_metadata.expect("group metadata");
                groups.push((
                    hex::encode(&group.id),
                    group.created_at_ns.to_string(),
                    mutable.attributes["group_name"].clone(),
                    mutable.attributes["description"].clone(),
                ));
            }
            Element::GroupMessage(message) => messages.push((
                hex::encode(&message.id),
                hex::encode(&message.group_id),
                message.sender_inbox_id,
                message.sent_at_ns.to_string(),
                hex::encode(&message.decrypted_message_bytes),
            )),
            Element::Consent(record) => consent.push((
                match record.entity_type() {
                    ConsentTypeSave::ConversationId => "conversation_id",
                    ConsentTypeSave::InboxId => "inbox_id",
                    other => panic!("unexpected consent entity type {other:?}"),
                },
                record.entity.clone(),
                match record.state() {
                    ConsentStateSave::Allowed => "allowed",
                    ConsentStateSave::Denied => "denied",
                    other => panic!("unexpected consent state {other:?}"),
                },
            )),
            other => panic!("unexpected archive element {other:?}"),
        }
    }
    let expected_groups = expected["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| {
            (
                text(&group["id"]),
                text(&group["createdAtNs"]),
                text(&group["name"]),
                text(&group["description"]),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, expected_groups);
    // The archive does not promise message order; compare by message ID.
    let mut expected_messages = expected["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|message| {
            (
                text(&message["id"]),
                text(&message["groupId"]),
                text(&message["senderInboxId"]),
                text(&message["sentAtNs"]),
                text(&message["encodedContentHex"]),
            )
        })
        .collect::<Vec<_>>();
    messages.sort();
    expected_messages.sort();
    assert_eq!(messages, expected_messages);
    let mut expected_consent = expected["consent"]
        .as_array()
        .expect("consent")
        .iter()
        .map(|record| {
            let kind = match record["entityType"].as_str() {
                Some("conversation_id") => "conversation_id",
                Some("inbox_id") => "inbox_id",
                other => panic!("unexpected fixture entity type {other:?}"),
            };
            let state = match record["state"].as_str() {
                Some("allowed") => "allowed",
                Some("denied") => "denied",
                other => panic!("unexpected fixture state {other:?}"),
            };
            (kind, text(&record["entity"]), state)
        })
        .collect::<Vec<_>>();
    consent.sort();
    expected_consent.sort();
    assert_eq!(consent, expected_consent);

    // A wrong key fails as input and publishes nothing over the archive.
    let archive = fs::read(&args.output_path)?;
    let mut wrong = args.clone();
    wrong.database_key = Some(vec![0; 32]);
    wrong.output_path = format!("{}.wrong-key", args.output_path);
    assert!(matches!(
        prepare_migration_archive(wrong.clone()).await,
        Err(MigrationError::InvalidInput(_))
    ));
    assert!(!Path::new(&wrong.output_path).exists());
    assert_eq!(fs::read(&args.output_path)?, archive);
    assert_eq!(source_bytes(&args), before);
}

// verifies: MIG-002, MIG-003, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn encrypted_wal_keys_and_source_bytes() {
    let (_directory, args) = fixture("encrypted.db3");
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (2, 4, 1)
    );
    let records = elements(&report.archive_path).await;
    assert!(records.iter().any(|e| matches!(e, Element::GroupMessage(m) if m.id == vec![9;32] && m.sent_at_ns == 1700000000000000124)));
    assert_eq!(source_bytes(&args), before);
    let archive = fs::read(&args.output_path)?;
    let mut wrong = args.clone();
    wrong.database_key = Some(vec![0x33; 32]);
    assert!(matches!(
        prepare_migration_archive(wrong).await,
        Err(MigrationError::InvalidInput(_))
    ));
    assert_eq!(source_bytes(&args), before);
    assert_eq!(fs::read(&args.output_path)?, archive);
    for size in [0, 31, 33] {
        let mut wrong = args.clone();
        wrong.archive_key = vec![7; size];
        assert!(matches!(
            prepare_migration_archive(wrong).await,
            Err(MigrationError::InvalidInput(_))
        ));
    }
    assert_eq!(fs::read(&args.output_path)?, archive);
}

// verifies: MIG-001, MIG-002, MIG-006
#[xmtp_common::test(unwrap_try = true)]
async fn earlier_schema_exports_history_without_recorded_deadlines() {
    let (_directory, args) = fixture("early.db3");
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (2, 5, 1)
    );
    let records = elements(&report.archive_path).await;
    assert_eq!(records.len(), 8);
    let ids: Vec<_> = records
        .into_iter()
        .filter_map(|record| match record {
            Element::GroupMessage(message) => Some(message.id),
            _ => None,
        })
        .collect();
    assert_eq!(ids, [1, 2, 3, 8, 10].map(|id| vec![id; 32]));
    assert_eq!(source_bytes(&args), before);
}

// verifies: MIG-005
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_optional_metadata_keeps_history() {
    let (_directory, args) = fixture("stable.db3");
    edit(&args, "UPDATE openmls_key_value SET value_bytes=x'ff'");
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!((report.group_count, report.message_count), (2, 3));
    for element in elements(&report.archive_path).await {
        if let Element::Group(group) = element {
            assert!(group.metadata.is_none());
            assert!(group.mutable_metadata.is_none());
        }
    }
}

// verifies: MIG-002, MIG-004, MIG-005, ARCH-007
#[xmtp_common::test(unwrap_try = true)]
async fn required_records_and_schema_fail_without_replacing_output() {
    let mut failures = vec![];
    for (sql, expected) in [
        (
            "UPDATE groups SET id=x'' WHERE id=x'44444444444444444444444444444444'",
            ExpectedError::RecordRead,
        ),
        (
            "DELETE FROM groups WHERE id=x'44444444444444444444444444444444'",
            ExpectedError::RecordRead,
        ),
        (
            "UPDATE groups SET id='DDDDDDDDDDDDDDDD' WHERE id=x'44444444444444444444444444444444'; UPDATE group_messages SET group_id='DDDDDDDDDDDDDDDD' WHERE group_id=x'44444444444444444444444444444444'",
            ExpectedError::RecordRead,
        ),
        (
            "UPDATE consent_records SET entity_type=99",
            ExpectedError::RecordRead,
        ),
        (
            "INSERT INTO __diesel_schema_migrations(version) VALUES('99999999999999')",
            ExpectedError::UnsupportedSchema,
        ),
        (
            "DROP TABLE __diesel_schema_migrations",
            ExpectedError::UnsupportedSchema,
        ),
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(&args, sql);
        failures.extend(rejects_without_publishing(&args, directory.path(), sql, expected).await);
    }
    assert!(
        failures.is_empty(),
        "wrong record or schema rejection: {failures:?}"
    );
}

// verifies: MIG-002, MIG-003, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn oversized_migration_history_preserves_source_and_output() {
    let mut failures = vec![];
    for sql in [
        "INSERT INTO __diesel_schema_migrations(version) VALUES (CAST(zeroblob(1048576) AS TEXT))",
        "WITH RECURSIVE rows(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM rows WHERE n<100000) INSERT INTO __diesel_schema_migrations(version) SELECT printf('unknown-%08d', n) FROM rows",
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(&args, sql);
        failures.extend(
            rejects_without_publishing(
                &args,
                directory.path(),
                sql,
                ExpectedError::UnsupportedSchema,
            )
            .await,
        );
    }
    assert!(
        failures.is_empty(),
        "oversized history changed source or output: {failures:?}"
    );
}

// verifies: MIG-002
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn source_aliases_and_other_process_sqlite_locks_are_rejected() {
    let (_directory, args) = fixture("stable.db3");
    let before = source_bytes(&args);
    let mut alias = args.clone();
    alias.output_path = args.database_path.clone();
    assert!(matches!(
        prepare_migration_archive(alias).await,
        Err(MigrationError::InvalidInput(_))
    ));
    std::os::unix::fs::symlink(&args.database_path, &args.output_path)?;
    assert!(matches!(
        prepare_migration_archive(args.clone()).await,
        Err(MigrationError::InvalidInput(_))
    ));
    fs::remove_file(&args.output_path)?;
    fs::hard_link(&args.database_path, &args.output_path)?;
    assert!(matches!(
        prepare_migration_archive(args.clone()).await,
        Err(MigrationError::InvalidInput(_))
    ));
    fs::remove_file(&args.output_path)?;
    assert_eq!(source_bytes(&args), before);
    for mode in ["DELETE", "WAL"] {
        let mut child = Command::new("python3").args(["-c", "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('PRAGMA journal_mode='+sys.argv[2]); c.execute('BEGIN IMMEDIATE'); print('locked',flush=True); sys.stdin.read(1); c.rollback(); c.close()", &args.database_path, mode])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut ready)?;
        assert_eq!(ready.trim(), "locked");
        let before = source_bytes(&args);
        let result = prepare_migration_archive(args.clone()).await;
        let after = source_bytes(&args);
        child.stdin.take().unwrap().write_all(b"x")?;
        assert!(child.wait()?.success());
        assert!(
            matches!(result, Err(MigrationError::SourceBusy)),
            "{result:?}"
        );
        assert_eq!(after, before);
    }
}

// verifies: MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn failed_and_cancelled_output_preserves_completed_archive() {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("history.xmtp");
    fs::write(&path, b"completed archive")?;
    for cancel_before_publish in [false, true] {
        let cancel = CancellationToken::new();
        let result: Result<(), MigrationError> = write_output(&path, &cancel, |sink| {
            sink.write_all(b"incomplete bytes").map_err(output)?;
            if cancel_before_publish {
                cancel.cancel();
                Ok(())
            } else {
                Err(output(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "injected failed write",
                )))
            }
        });
        assert!(matches!(result, Err(MigrationError::Output(_))));
        assert_eq!(fs::read(&path)?, b"completed archive");
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    }
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel::<()>();
    let target = path.clone();
    let mut future = Box::pin(offload(move |cancel| {
        let _finished = finished;
        write_output(&target, cancel, |sink| {
            sink.write_all(b"incomplete bytes").map_err(output)?;
            entered.send(()).unwrap();
            gate.recv().unwrap();
            Ok(())
        })
    }));
    assert!(futures::poll!(future.as_mut()).is_pending());
    ready.await?;
    drop(future);
    release.send(())?;
    assert!(done.await.is_err());
    assert_eq!(fs::read(&path)?, b"completed archive");
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
}

// verifies: MIG-002, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn failed_legacy_migration_preserves_source_and_output() {
    let (directory, args) = fixture("early.db3");
    edit(
        &args,
        "ALTER TABLE group_messages ADD COLUMN expire_at_ns BIGINT",
    );
    let before = source_bytes(&args);
    fs::write(&args.output_path, b"completed archive")?;
    assert!(matches!(
        prepare_migration_archive(args.clone()).await,
        Err(MigrationError::Migration(_))
    ));
    assert_eq!(source_bytes(&args), before);
    assert_eq!(fs::read(&args.output_path)?, b"completed archive");
    assert_eq!(fs::read_dir(directory.path())?.count(), 2);
}

// verifies: MIG-001, MIG-006
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_migration_timestamps_do_not_remove_null_expiry_history() {
    for time in ["not-a-timestamp", "999999999999999999999999", ""] {
        let (_directory, args) = fixture("stable.db3");
        edit(
            &args,
            &format!(
                "UPDATE __diesel_schema_migrations SET run_on='{time}' WHERE version='20250717111748'"
            ),
        );
        let before = source_bytes(&args);
        let report = prepare_migration_archive(args.clone()).await?;
        assert_eq!(
            (
                report.group_count,
                report.message_count,
                report.consent_count
            ),
            (2, 3, 1)
        );
        assert_eq!(source_bytes(&args), before);
    }
}

// verifies: MIG-003, CONS-050
#[xmtp_common::test(unwrap_try = true)]
async fn consent_states_match_the_archive_wire_contract() {
    use xmtp_proto::xmtp::device_sync::consent_backup::ConsentStateSave;

    let (_directory, args) = fixture("consent-states.db3");
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(report.consent_count, 3);
    assert_eq!(source_bytes(&args), before);
    let consents: Vec<_> = elements(&report.archive_path)
        .await
        .into_iter()
        .filter_map(|element| match element {
            Element::Consent(consent) => Some(consent),
            _ => None,
        })
        .collect();
    for (consent, (prefix, state, wire)) in consents.iter().zip([
        ("02", ConsentStateSave::Unknown, 1),
        ("03", ConsentStateSave::Allowed, 2),
        ("04", ConsentStateSave::Denied, 3),
    ]) {
        assert_eq!(consent.entity, prefix.repeat(32));
        assert_eq!(consent.entity_type, 2);
        assert_eq!(consent.state, wire);
        assert_eq!(consent.state(), state);
        assert_eq!(consent.consented_at_ns, 1700000000000000009);
    }
    assert_eq!(consents.len(), 3);
}

/// With an identity row, a DM's consent rows and its peer's inbox row become
/// one inbox record with the strongest state at the greatest time, including a
/// DM whose group the archive leaves out. Without one, rows are written as stored.
// verifies: MIG-009, CONS-061, CONS-050
#[xmtp_common::test(unwrap_try = true)]
async fn dm_consent_rows_are_written_as_their_peer() {
    use xmtp_proto::xmtp::device_sync::consent_backup::{ConsentStateSave, ConsentTypeSave};
    let restored_dm = "99".repeat(16);
    let consents = |identity: bool, inbox_state: i32, dm_state: i32| {
        let (directory, args) = fixture("stable.db3");
        let mut sql = format!(
            "UPDATE consent_records SET state = {inbox_state}; \
             INSERT INTO consent_records VALUES (1, {dm_state}, '{}', 10); \
             INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, \
               added_by_inbox_id, conversation_type, dm_id) \
               VALUES (x'{restored_dm}', 1, 4, 0, '{OWNER}', 2, 'dm:{OWNER}:{PEER}'); \
             INSERT INTO consent_records VALUES (1, 1, '{restored_dm}', 20);",
            GROUP.to_uppercase()
        );
        if identity {
            sql += &format!(
                "INSERT INTO identity (inbox_id, installation_keys, credential_bytes, rowid) \
                 VALUES ('{OWNER}', x'00', x'00', 1);"
            );
        }
        edit(&args, &sql);
        async move {
            let report = prepare_migration_archive(args).await.unwrap();
            let records: Vec<_> = elements(&report.archive_path)
                .await
                .into_iter()
                .filter_map(|element| match element {
                    Element::Consent(consent) => Some((
                        consent.entity_type(),
                        consent.entity.to_lowercase(),
                        consent.state(),
                        consent.consented_at_ns,
                    )),
                    _ => None,
                })
                .collect();
            assert_eq!(report.consent_count, records.len() as u64);
            drop(directory);
            records
        }
    };
    let inbox = |state| {
        (
            ConsentTypeSave::InboxId,
            PEER.to_string(),
            state,
            1700000000000000009,
        )
    };
    // A DM deny beats a later-created inbox allow.
    assert_eq!(
        consents(true, 1, 2).await,
        [inbox(ConsentStateSave::Denied)]
    );
    // An inbox deny is kept over a DM allow.
    assert_eq!(
        consents(true, 2, 1).await,
        [inbox(ConsentStateSave::Denied)]
    );
    assert_eq!(
        consents(true, 0, 1).await,
        [inbox(ConsentStateSave::Allowed)]
    );
    // Without an identity row no row moves.
    let stored = consents(false, 1, 2).await;
    assert_eq!(stored.len(), 3, "{stored:?}");
    assert!(stored.contains(&(
        ConsentTypeSave::ConversationId,
        GROUP.into(),
        ConsentStateSave::Denied,
        10
    )));
}

// verifies: MIG-002, MIG-005
#[xmtp_common::test(unwrap_try = true)]
async fn oversized_optional_context_keeps_history_and_source() {
    let (_directory, args) = fixture("stable.db3");
    let mut bytes = include_bytes!("../../fixtures/appdata-context.bincode").to_vec();
    bytes.resize(1024 * 1024 + 1, 0);
    let mut conn = SqliteConnection::establish(&args.database_path)?;
    diesel::sql_query("UPDATE openmls_key_value SET value_bytes = ?")
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .execute(&mut conn)?;
    drop(conn);
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!((report.group_count, report.message_count), (2, 3));
    assert_eq!(source_bytes(&args), before);
    for record in elements(&report.archive_path).await {
        if let Element::Group(group) = record {
            assert!(group.metadata.is_none());
            assert!(group.mutable_metadata.is_none());
        }
    }
}

// verifies: MIG-002, MIG-004
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
fn special_source_files_are_rejected_without_blocking() {
    use std::{
        os::unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt},
        },
        time::{Duration, Instant},
    };
    const PROBE_PATH: &str = "XMTP_MIGRATION_SPECIAL_FILE_PROBE";
    const PROBE_DIRECT: &str = "XMTP_MIGRATION_SPECIAL_FILE_DIRECT";
    if let Some(path) = std::env::var_os(PROBE_PATH) {
        let path = PathBuf::from(path);
        if std::env::var_os(PROBE_DIRECT).is_some() {
            // The caller saw a regular file before this path was replaced.
            assert!(open_regular_source(&path, true).is_err());
        } else {
            let output = path.parent().unwrap().join("history.xmtp");
            assert!(matches!(
                working_copy(&path, &output, &CancellationToken::new()),
                Err(MigrationError::InvalidInput(_))
            ));
        }
        return;
    }
    let mut failures = vec![];
    for direct in [false, true] {
        for suffix in SIDECARS {
            let (_directory, args) = fixture("stable.db3");
            let source = Path::new(&args.database_path);
            let special = sidecar(source, suffix);
            let before = source_bytes(&args);
            fs::write(&args.output_path, b"completed archive")?;
            // Check the regular path, then replace it before the open probe.
            fs::write(&special, b"regular file")?;
            assert!(fs::metadata(&special)?.is_file());
            fs::remove_file(&special)?;
            let name = std::ffi::CString::new(special.as_os_str().as_bytes())?;
            // SAFETY: name is a live, NUL-terminated path. The mode is valid.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            let inode = fs::metadata(&special)?.ino();
            let mut command = Command::new(std::env::current_exe()?);
            command
                .args([
                    "--exact",
                    "native::tests::special_source_files_are_rejected_without_blocking",
                    "--nocapture",
                ])
                .env(PROBE_PATH, if direct { &special } else { source });
            if direct {
                command.env(PROBE_DIRECT, "1");
            }
            let mut child = command.spawn()?;
            let deadline = Instant::now() + Duration::from_secs(3);
            let passed = loop {
                if let Some(status) = child.try_wait()? {
                    break status.success();
                }
                if Instant::now() >= deadline {
                    child.kill()?;
                    child.wait()?;
                    break false;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            if !passed {
                failures.push(format!("suffix={suffix:?}, direct={direct}"));
            }
            let metadata = fs::metadata(&special)?;
            assert!(metadata.file_type().is_fifo());
            assert_eq!(metadata.ino(), inode);
            for (other_suffix, bytes) in &before {
                if other_suffix != suffix {
                    assert_eq!(fs::read(sidecar(source, other_suffix))?, *bytes);
                }
            }
            assert_eq!(fs::read(&args.output_path)?, b"completed archive");
        }
    }
    assert!(
        failures.is_empty(),
        "special source was accepted or blocked: {failures:?}"
    );
}

// verifies: MIG-002, MIG-003, MIG-004
#[xmtp_common::test(unwrap_try = true)]
fn salt_sidecar_size_is_checked_before_copying() {
    for size in [0, 31, 32, 33, 1024 * 1024] {
        let (_directory, args) = fixture("encrypted.db3");
        let source = Path::new(&args.database_path);
        let salt = sidecar(source, ".sqlcipher_salt");
        fs::write(&salt, vec![b'2'; size])?;
        let before = source_bytes(&args);
        fs::write(&args.output_path, b"completed archive")?;
        let result = working_copy(
            source,
            Path::new(&args.output_path),
            &CancellationToken::new(),
        );
        if size == 32 {
            let (copy, _) = result?;
            assert_eq!(
                fs::read(sidecar(&copy.path().join("source.db3"), ".sqlcipher_salt"))?,
                before[".sqlcipher_salt"]
            );
        } else {
            assert!(
                matches!(result, Err(MigrationError::InvalidInput(_))),
                "accepted {size} salt bytes"
            );
        }
        assert_eq!(source_bytes(&args), before);
        assert_eq!(fs::read(&args.output_path)?, b"completed archive");
    }
}

// verifies: MIG-003
#[xmtp_common::test(unwrap_try = true)]
fn salt_reads_are_bounded_even_if_the_file_grows() {
    struct Measured {
        remaining: usize,
        read: usize,
        largest_request: usize,
    }
    impl Read for Measured {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.largest_request = self.largest_request.max(output.len());
            let count = output.len().min(self.remaining);
            output[..count].fill(b'2');
            self.remaining -= count;
            self.read += count;
            Ok(count)
        }
    }
    for size in [0, 31, 32, 33, 1024 * 1024] {
        let mut source = Measured {
            remaining: size,
            read: 0,
            largest_request: 0,
        };
        let result = read_salt(&mut source);
        if size == 32 {
            assert_eq!(result?, [b'2'; 32]);
        } else {
            assert!(matches!(result, Err(MigrationError::InvalidInput(_))));
        }
        assert!(source.read <= 33, "read {} salt bytes", source.read);
        assert!(
            source.largest_request <= 32,
            "requested {} salt bytes",
            source.largest_request
        );
    }
}

// verifies: MIG-002, MIG-004, MIG-005
#[xmtp_common::test(unwrap_try = true)]
async fn null_eligibility_fields_fail_without_publishing() {
    let mut failures = vec![];
    for (table, field, id) in [
        ("groups", "conversation_type", GROUP.to_owned()),
        ("groups", "membership_state", GROUP.to_owned()),
        ("group_messages", "kind", "01".repeat(32)),
    ] {
        let (directory, args) = fixture("stable.db3");
        // Rebuild this corrupt table without NOT NULL constraints.
        edit(
            &args,
            &format!(
                "DROP TRIGGER IF EXISTS msg_inserted; DROP VIEW IF EXISTS conversation_list; CREATE TABLE nullable_copy AS SELECT * FROM {table}; DROP TABLE {table}; ALTER TABLE nullable_copy RENAME TO {table}; UPDATE {table} SET {field}=NULL WHERE id=x'{id}';"
            ),
        );
        failures.extend(
            rejects_without_publishing(
                &args,
                directory.path(),
                &format!("{table}.{field}"),
                ExpectedError::RecordRead,
            )
            .await,
        );
    }
    assert!(
        failures.is_empty(),
        "unknown eligibility silently removed history: {failures:?}"
    );
}

// verifies: MIG-001, MIG-002, MIG-004
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn source_symlinks_preserve_wal_and_salt() {
    for supplied_sidecars in [false, true] {
        let (directory, original) = fixture("encrypted.db3");
        let aliases = directory.path().join("aliases");
        fs::create_dir(&aliases)?;
        let supplied = aliases.join("legacy.db3");
        std::os::unix::fs::symlink(&original.database_path, &supplied)?;
        if supplied_sidecars {
            for suffix in SIDECARS.iter().filter(|suffix| !suffix.is_empty()) {
                let from = sidecar(Path::new(&original.database_path), suffix);
                if from.exists() {
                    fs::rename(from, sidecar(&supplied, suffix))?;
                }
            }
        }
        let mut args = original.clone();
        args.database_path = supplied.to_str().unwrap().to_owned();
        let before_original = source_bytes(&original);
        let before_supplied = source_bytes(&args);
        let report = prepare_migration_archive(args.clone()).await?;
        assert_eq!(
            (
                report.group_count,
                report.message_count,
                report.consent_count
            ),
            (2, 4, 1)
        );
        let records = elements(&report.archive_path).await;
        assert!(records.iter().any(
            |record| matches!(record, Element::GroupMessage(message) if message.id == vec![9; 32])
        ));
        assert_eq!(source_bytes(&original), before_original);
        assert_eq!(source_bytes(&args), before_supplied);
    }
    let (directory, original) = fixture("encrypted.db3");
    let supplied = directory.path().join("alias.db3");
    std::os::unix::fs::symlink(&original.database_path, &supplied)?;
    fs::copy(
        sidecar(Path::new(&original.database_path), "-wal"),
        sidecar(&supplied, "-wal"),
    )?;
    let mut args = original.clone();
    args.database_path = supplied.to_str().unwrap().to_owned();
    fs::write(&args.output_path, b"completed archive")?;
    let before_original = source_bytes(&original);
    let before_supplied = source_bytes(&args);
    let before_entries = entries(directory.path());
    assert!(matches!(
        prepare_migration_archive(args.clone()).await,
        Err(MigrationError::InvalidInput(_))
    ));
    assert_eq!(source_bytes(&original), before_original);
    assert_eq!(source_bytes(&args), before_supplied);
    assert_eq!(fs::read(&args.output_path)?, b"completed archive");
    assert_eq!(entries(directory.path()), before_entries);
}

// verifies: MIG-004
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn symlink_sidecars_are_rejected_before_copying() {
    for suffix in SIDECARS.iter().filter(|suffix| !suffix.is_empty()) {
        let (directory, args) = fixture("encrypted.db3");
        let foreign = tempfile::tempdir()?;
        let source_sidecar = sidecar(Path::new(&args.database_path), suffix);
        let foreign_sidecar = foreign.path().join("other-sidecar");
        if source_sidecar.exists() {
            fs::rename(&source_sidecar, &foreign_sidecar)?;
        } else {
            fs::write(&foreign_sidecar, [])?;
        }
        std::os::unix::fs::symlink(&foreign_sidecar, &source_sidecar)?;
        let before = source_bytes(&args);
        let foreign_before = fs::read(&foreign_sidecar)?;
        fs::write(&args.output_path, b"completed archive")?;
        let before_entries = entries(directory.path());
        assert!(
            matches!(
                prepare_migration_archive(args.clone()).await,
                Err(MigrationError::InvalidInput(_))
            ),
            "symlink sidecar was accepted: {suffix}"
        );
        // Also cover a path replaced after the caller inspected its type.
        assert!(open_regular_source(&source_sidecar, false).is_err());
        assert!(
            fs::symlink_metadata(&source_sidecar)?
                .file_type()
                .is_symlink()
        );
        assert_eq!(source_bytes(&args), before);
        assert_eq!(fs::read(&foreign_sidecar)?, foreign_before);
        assert_eq!(fs::read(&args.output_path)?, b"completed archive");
        assert_eq!(entries(directory.path()), before_entries);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn dropped_call_stops_source_copy() {
    struct GatedWriter {
        file: fs::File,
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        gate: std::sync::mpsc::Receiver<()>,
    }
    impl Write for GatedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = self.file.write(bytes)?;
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.gate.recv().unwrap();
            }
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }
    let source = vec![0x55; 1024 * 1024];
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    let mut future = Box::pin(offload(move |cancel| {
        let temporary = tempfile::tempdir().map_err(input)?;
        let copied = temporary.path().join("source.db3");
        let file = fs::File::create(&copied).map_err(input)?;
        let result = copy_source(
            source.as_slice(),
            GatedWriter {
                file,
                entered: Some(entered),
                gate,
            },
            cancel,
        );
        let count = fs::metadata(&copied).map_err(input)?.len();
        finished.send((result.is_err(), count)).unwrap();
        result
    }));
    assert!(futures::poll!(future.as_mut()).is_pending());
    ready.await?;
    drop(future);
    release.send(())?;
    let (failed, copied) = done.await?;
    assert!(failed, "cancelled source copy completed");
    assert!(
        copied <= 64 * 1024,
        "copied {copied} bytes after cancellation"
    );
}

// verifies: MIG-002, MIG-003, MIG-004
#[xmtp_common::test(unwrap_try = true)]
fn temporary_copy_failures_keep_source_errors_separate() {
    const PROBE_SOURCE: &str = "XMTP_MIGRATION_TEMP_ERROR_SOURCE";
    if let Some(source) = std::env::var_os(PROBE_SOURCE) {
        let source = PathBuf::from(source);
        let output = source.parent().unwrap().join("history.xmtp");
        let result = working_copy(&source, &output, &CancellationToken::new());
        assert!(
            matches!(result, Err(MigrationError::Output(_))),
            "temporary directory failure: {result:?}"
        );
        return;
    }
    struct Failure;
    impl Read for Failure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected source read failure"))
        }
    }
    impl Write for Failure {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected destination write failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let token = CancellationToken::new();
    assert!(matches!(
        copy_source(Failure, io::sink(), &token),
        Err(MigrationError::InvalidInput(_))
    ));
    let mut failures = vec![];
    let result = copy_source(b"source bytes".as_slice(), Failure, &token);
    if !matches!(result, Err(MigrationError::Output(_))) {
        failures.push(format!("temporary write: {result:?}"));
    }
    #[cfg(unix)]
    {
        let (directory, args) = fixture("stable.db3");
        let before = source_bytes(&args);
        fs::write(&args.output_path, b"completed archive")?;
        let invalid_temp = directory.path().join("not-a-directory");
        fs::write(&invalid_temp, b"file")?;
        let status = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "native::tests::temporary_copy_failures_keep_source_errors_separate",
                "--nocapture",
            ])
            .env(PROBE_SOURCE, &args.database_path)
            .env("TMPDIR", &invalid_temp)
            .status()?;
        if !status.success() {
            failures.push("temporary directory failure had the wrong category".into());
        }
        assert_eq!(source_bytes(&args), before);
        assert_eq!(fs::read(&args.output_path)?, b"completed archive");
    }
    assert!(
        failures.is_empty(),
        "wrong temporary-storage failure category: {failures:?}"
    );
}

// verifies: MIG-002, MIG-004, MIG-006
#[xmtp_common::test(unwrap_try = true)]
async fn invalid_message_kinds_fail_without_publishing() {
    let mut failures = vec![];
    for kind in [-1, 0, 3, 99] {
        for orphan in [false, true] {
            let (directory, args) = fixture("stable.db3");
            let group = if orphan {
                format!(", group_id=x'{}'", "ff".repeat(16))
            } else {
                String::new()
            };
            edit(
                &args,
                &format!(
                    "UPDATE group_messages SET kind={kind}{group} WHERE id=x'{}';",
                    "01".repeat(32)
                ),
            );
            failures.extend(
                rejects_without_publishing(
                    &args,
                    directory.path(),
                    &format!("kind={kind}, orphan={orphan}"),
                    ExpectedError::RecordRead,
                )
                .await,
            );
        }
    }
    // Membership changes are known records that migration must exclude.
    let (_directory, args) = fixture("stable.db3");
    edit(
        &args,
        &format!(
            "UPDATE group_messages SET kind=2 WHERE id=x'{}';",
            "01".repeat(32)
        ),
    );
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(report.message_count, 2);
    assert_eq!(source_bytes(&args), before);
    assert!(
        failures.is_empty(),
        "unsupported kinds silently removed history: {failures:?}"
    );
}

mod group_consent_sizes;
mod message_sizes;

mod cancellation;

mod deletions;

#[cfg(unix)]
mod output_path;
