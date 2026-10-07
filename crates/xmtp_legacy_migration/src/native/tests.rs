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
        }
        let Element::Consent(consent) = &records[5] else {
            panic!("missing consent")
        };
        assert_eq!(consent.entity, PEER);
        assert_eq!(consent.state, 2);
        assert_eq!(consent.consented_at_ns, 1700000000000000009);
    }
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
    assert_eq!(elements(&report.archive_path).await.len(), 8);
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
    for sql in [
        "UPDATE groups SET id=x'' WHERE id=x'44444444444444444444444444444444'",
        "DELETE FROM groups WHERE id=x'44444444444444444444444444444444'",
        "UPDATE groups SET id='DDDDDDDDDDDDDDDD' WHERE id=x'44444444444444444444444444444444'; UPDATE group_messages SET group_id='DDDDDDDDDDDDDDDD' WHERE group_id=x'44444444444444444444444444444444'",
        "UPDATE consent_records SET entity_type=99",
        "INSERT INTO __diesel_schema_migrations(version) VALUES('99999999999999')",
        "DROP TABLE __diesel_schema_migrations",
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(&args, sql);
        fs::write(&args.output_path, b"completed archive")?;
        let before = source_bytes(&args);
        let result = prepare_migration_archive(args.clone()).await;
        assert!(
            matches!(
                result,
                Err(MigrationError::RecordRead(_)) | Err(MigrationError::UnsupportedSchema)
            ),
            "{result:?}"
        );
        assert_eq!(source_bytes(&args), before);
        assert_eq!(fs::read(&args.output_path)?, b"completed archive");
        assert_eq!(fs::read_dir(directory.path())?.count(), 2);
    }
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

// verifies: MIG-003, CONS-002
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
            assert!(open_regular_source(&path).is_err());
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
            let copy = result?;
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
        let (_directory, args) = fixture("stable.db3");
        // Rebuild this corrupt table without NOT NULL constraints.
        edit(
            &args,
            &format!(
                "DROP TRIGGER IF EXISTS msg_inserted; DROP VIEW IF EXISTS conversation_list; CREATE TABLE nullable_copy AS SELECT * FROM {table}; DROP TABLE {table}; ALTER TABLE nullable_copy RENAME TO {table}; UPDATE {table} SET {field}=NULL WHERE id=x'{id}';"
            ),
        );
        let before = source_bytes(&args);
        fs::write(&args.output_path, b"completed archive")?;
        let result = prepare_migration_archive(args.clone()).await;
        if !matches!(result, Err(MigrationError::RecordRead(_))) {
            failures.push(format!("{table}.{field}: {result:?}"));
        } else {
            assert_eq!(fs::read(&args.output_path)?, b"completed archive");
        }
        assert_eq!(source_bytes(&args), before);
    }
    assert!(
        failures.is_empty(),
        "unknown eligibility silently removed history: {failures:?}"
    );
}

// verifies: MIG-002, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn dropped_call_stops_source_copy_and_removes_partial_copy() {
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
    let directory = tempfile::tempdir()?;
    let output_path = directory.path().join("history.xmtp");
    fs::write(&output_path, b"completed archive")?;
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
        let partial = temporary.path().to_owned();
        drop(temporary);
        finished
            .send((result.is_err(), count, partial.exists()))
            .unwrap();
        result
    }));
    assert!(futures::poll!(future.as_mut()).is_pending());
    ready.await?;
    drop(future);
    release.send(())?;
    let (failed, copied, partial_exists) = done.await?;
    assert!(failed, "cancelled source copy completed");
    assert!(
        copied <= 64 * 1024,
        "copied {copied} bytes after cancellation"
    );
    assert!(!partial_exists);
    assert_eq!(fs::read(&output_path)?, b"completed archive");
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
            for previous in [false, true] {
                let (_directory, args) = fixture("stable.db3");
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
                let before = source_bytes(&args);
                if previous {
                    fs::write(&args.output_path, b"completed archive")?;
                }
                let result = prepare_migration_archive(args.clone()).await;
                if !matches!(result, Err(MigrationError::RecordRead(_))) {
                    failures.push(format!(
                        "kind={kind}, orphan={orphan}, previous={previous}: {result:?}"
                    ));
                } else if previous {
                    assert_eq!(fs::read(&args.output_path)?, b"completed archive");
                } else {
                    assert!(!Path::new(&args.output_path).exists());
                }
                assert_eq!(source_bytes(&args), before);
            }
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
