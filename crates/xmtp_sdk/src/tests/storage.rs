use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
async fn storage_options_debug_redacts_direct_and_nested_key() {
    let key = vec![193, 47, 128, 219];
    let storage = crate::StorageOptions {
        encryption_key: Some(key.clone()),
        label: Some("saved-store".into()),
        ..Default::default()
    };
    for text in [format!("{storage:?}"), format!("{:?}", Some(&storage))] {
        assert!(
            !text.contains("193"),
            "storage diagnostic contains key bytes"
        );
        assert!(text.contains("[redacted]"));
        assert!(text.contains("saved-store"));
    }
    assert_eq!(storage.encryption_key, Some(key));
    assert!(format!("{:?}", crate::StorageOptions::default()).contains("encryption_key: None"));
}

// verifies: STORE-009
#[xmtp_common::test(unwrap_try = true)]
async fn storage_path_keeps_opened_relative_file_after_chdir() {
    struct RestoreDirectory(std::path::PathBuf);

    impl Drop for RestoreDirectory {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore working directory");
        }
    }

    let original = std::env::current_dir()?;
    let relative = std::path::PathBuf::from(format!(
        "target/sdk-relative-storage-{}-{}/db.sqlite",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    std::fs::create_dir_all(relative.parent().expect("database directory"))?;
    let expected = std::path::absolute(&relative)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&relative);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    assert!(expected.is_file());
    {
        let _restore = RestoreDirectory(original);
        std::env::set_current_dir(std::env::temp_dir())?;
        assert_eq!(
            client.storage().path().await?,
            Some(expected.to_string_lossy().into_owned())
        );
    }
    client.end().await?;
    std::fs::remove_dir_all(relative.parent().expect("database directory"))?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_delete_ends_event_reader_and_listener() {
    let directory = temp_root("delete-events");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("client.sqlite");
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    let (listener, _) = event_probe(None, false, None, false);
    client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), listener)
        .await?;
    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    client.storage().delete().await?;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await???
            .is_none()
    );
    assert_eq!(client.listeners.active_count_for_test(), 0);
    client.end().await?;
    assert!(!path.exists());
    std::fs::remove_dir_all(directory)?;
}

// verifies: STORE-017
#[xmtp_common::test(unwrap_try = true)]
async fn storage_delete_waits_for_running_call() {
    let directory = temp_root("delete-running");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("client.sqlite");
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    // The handoff gate holds next() inside its worker call.
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    group.send_text("held".into(), None).await?;
    let running_reader = reader.clone();
    let running = tokio::spawn(async move { running_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;

    let storage = client.storage();
    let mut deleting = tokio::spawn(async move { storage.delete().await });
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        while !client.inner.context.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(500), &mut deleting)
            .await
            .is_err(),
        "delete finished while a call was running"
    );
    assert!(path.exists(), "database file removed under a running call");
    assert!(matches!(
        group.messages(None).await,
        Err(XmtpError::ClientClosed(_))
    ));

    gate.release.notify_one();
    let read = xmtp_common::time::timeout(Duration::from_secs(10), running).await??;
    assert!(matches!(read, Ok(_) | Err(XmtpError::ClientClosed(_))));
    xmtp_common::time::timeout(Duration::from_secs(10), deleting).await???;
    assert!(!path.exists());
    std::fs::remove_dir_all(directory)?;
}

// A direct client call does not run on the SDK worker, so it enters the call
// gate itself. end() must wait for it and must not disconnect the database
// under it.
#[xmtp_common::test(unwrap_try = true)]
async fn end_waits_for_running_direct_call() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *client.call_gate.lock() = Some(gate.clone());
    let running_client = client.clone();
    let running = tokio::spawn(async move { running_client.inbox_state(true).await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;

    let ending_client = client.clone();
    let mut ending = tokio::spawn(async move { ending_client.end().await });
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        while !client.inner.context.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(500), &mut ending)
            .await
            .is_err(),
        "end finished while a direct call was running"
    );
    assert!(matches!(
        client.inbox_id_for(client.identity()).await,
        Err(XmtpError::ClientClosed(_))
    ));

    gate.release.notify_one();
    let state = xmtp_common::time::timeout(Duration::from_secs(10), running).await???;
    assert_eq!(state.inbox_id, client.inbox_id());
    xmtp_common::time::timeout(Duration::from_secs(10), ending).await???;
}

// verifies: STORE-018
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn storage_delete_can_retry_after_file_removal_fails() {
    use std::os::unix::fs::PermissionsExt;

    struct RestorePermissions {
        path: std::path::PathBuf,
        mode: u32,
    }

    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.mode))
                .expect("restore directory permissions");
        }
    }

    let directory = temp_root("delete-retry");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("client.sqlite");
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let original_mode = std::fs::metadata(&directory)?.permissions().mode();
    let restore = RestorePermissions {
        path: directory.clone(),
        mode: original_mode,
    };
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500))?;
    let error = client
        .storage()
        .delete()
        .await
        .expect_err("directory is read-only");
    drop(restore);
    assert!(path.exists());
    client.storage().delete().await?;
    assert!(!path.exists());
    client.end().await?;
    std::fs::remove_dir_all(directory)?;
    let XmtpError::Storage(details) = error else {
        panic!("file removal must report Storage: {error:?}");
    };
    assert_eq!(details.code, "Storage");
    assert!(matches!(details.category, crate::ErrorCategory::Storage));
    assert!(!details.retryable);
    assert!(!details.message.is_empty());
}

// verifies: STORE-018, ATCH-046
async fn check_storage_delete_files(
    directory_layout: bool,
    missing_main: bool,
    blocked_phase: Option<usize>,
) -> Result<(), xmtp_common::BoxDynError> {
    let root = temp_root("delete-family");
    std::fs::create_dir_all(&root)?;
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.single_connection = true;
    settings.storage.encryption_key = Some(vec![7; 32]);
    settings.storage.location = if directory_layout {
        StorageLocation::Directory {
            directory: root.to_string_lossy().into_owned(),
        }
    } else {
        explicit_location(&root.join("client.sqlite"))
    };
    let client = Client::create(signer.clone(), settings.clone()).await?;
    let path = std::path::PathBuf::from(client.storage().path().await?.unwrap());
    let attachments = if directory_layout {
        path.with_file_name("attachments")
    } else {
        path.with_extension("attachments")
    };
    client.end().await?;
    let salt = std::path::PathBuf::from(format!("{}.sqlcipher_salt", path.display()));
    let original_salt = std::fs::read(&salt)?;
    assert!(std::fs::metadata(&salt)?.permissions().readonly());
    let mut files = vec![path.clone()];
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        std::fs::write(&sidecar, suffix)?;
        files.push(sidecar);
    }
    files.push(salt.clone());
    files.push(salt.with_extension("sqlcipher_salt.tmp"));
    std::fs::write(&files[5], b"unfinished salt")?;
    let key = attachments.join("a".repeat(64));
    for dir in [
        &key,
        &attachments.join(".staged"),
        &attachments.join(".tmp"),
    ] {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("file"), b"local attachment bytes")?;
    }
    std::fs::create_dir_all(attachments.join("app"))?;
    let sentinel = attachments.join("app/sentinel");
    std::fs::write(&sentinel, b"keep")?;
    std::fs::write(attachments.join("A".repeat(64)), b"not a key")?;
    if missing_main {
        std::fs::remove_file(&path)?;
    }
    if let Some(phase) = blocked_phase {
        let blocked = if phase < files.len() {
            files[phase].clone()
        } else {
            key.clone()
        };
        let saved = blocked.with_extension("saved");
        std::fs::rename(&blocked, &saved)?;
        if phase < files.len() {
            std::fs::create_dir(&blocked)?;
        } else {
            std::fs::write(&blocked, b"not a directory")?;
        }
        let error = client
            .storage()
            .delete()
            .await
            .expect_err("required removal must fail");
        assert!(matches!(error, XmtpError::Storage(ref details)
            if matches!(details.category, crate::ErrorCategory::Storage) && !details.retryable));
        for later in files.iter().skip(phase + 1) {
            assert!(
                later.is_file(),
                "later file removed before retry: {}",
                later.display()
            );
        }
        assert_eq!(std::fs::read(&sentinel)?, b"keep");
        if phase < files.len() {
            assert!(
                key.join("file").is_file(),
                "attachments removed before database cleanup"
            );
            std::fs::remove_dir(&blocked)?;
        } else {
            std::fs::remove_file(&blocked)?;
        }
        std::fs::rename(saved, blocked)?;
    }
    let result = client.storage().delete().await;
    assert!(
        result.is_ok(),
        "deletion must continue past absent files: {result:?}"
    );
    for file in &files {
        assert!(
            !file.exists(),
            "database family remains: {}",
            file.display()
        );
    }
    for dir in [
        &key,
        &attachments.join(".staged"),
        &attachments.join(".tmp"),
    ] {
        assert!(
            !dir.exists(),
            "managed attachment directory remains: {}",
            dir.display()
        );
    }
    assert_eq!(std::fs::read(&sentinel)?, b"keep");
    assert_eq!(
        std::fs::read(attachments.join("A".repeat(64)))?,
        b"not a key"
    );
    assert!(attachments.is_dir());
    assert!(path.parent().unwrap().is_dir());
    client.storage().delete().await?;
    let recreated = Client::create(signer, settings).await?;
    assert_ne!(
        std::fs::read(salt)?,
        original_salt,
        "recreation reused the salt"
    );
    recreated.storage().delete().await?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

macro_rules! storage_delete_case {
    ($name:ident, $missing:expr, $blocked:expr) => {
        mod $name {
            use super::*;
            #[xmtp_common::test(unwrap_try = true)]
            async fn explicit() {
                check_storage_delete_files(false, $missing, $blocked).await?;
            }
            #[xmtp_common::test(unwrap_try = true)]
            async fn directory() {
                check_storage_delete_files(true, $missing, $blocked).await?;
            }
        }
    };
}
storage_delete_case!(storage_delete_complete_family, false, None);
storage_delete_case!(storage_delete_missing_main, true, None);
storage_delete_case!(storage_delete_failed_main, false, Some(0));
storage_delete_case!(storage_delete_failed_wal, false, Some(1));
storage_delete_case!(storage_delete_failed_shm, false, Some(2));
storage_delete_case!(storage_delete_failed_journal, false, Some(3));
storage_delete_case!(storage_delete_failed_salt, false, Some(4));
storage_delete_case!(storage_delete_failed_temporary_salt, false, Some(5));
storage_delete_case!(storage_delete_failed_attachments, false, Some(6));

#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_creates_its_database_directory_private() {
    use std::os::unix::fs::PermissionsExt;

    const CHILD: &str = "XMTP_TEST_PRIVATE_DIRECTORY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Set umask only in the child. Other tests keep their process settings.
        let marker = temp_root("storage-private-child-complete");
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg("umask 000; exec \"$@\"")
            .arg("storage-private")
            .arg(std::env::current_exe()?)
            .args([
                "tests::storage::explicit_storage_creates_its_database_directory_private",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, &marker)
            .status()?;
        assert!(status.success(), "private directory child failed");
        assert!(marker.is_file(), "private directory child did not run");
        std::fs::remove_file(marker)?;
        return;
    }

    // The attachments directory lies apart, so only the database creates its
    // parent. A normal directory proves that the child has a permissive umask.
    let root = temp_root("storage-private");
    std::fs::create_dir_all(&root)?;
    let control = root.join("control");
    std::fs::create_dir(&control)?;
    assert_eq!(
        std::fs::metadata(&control)?.permissions().mode() & 0o777,
        0o777
    );
    let parent = root.join("database");
    let mut settings = options();
    settings.storage.location = StorageLocation::Explicit {
        db_path: parent.join("client.sqlite").to_string_lossy().into_owned(),
        attachments_dir: root.join("attachments").to_string_lossy().into_owned(),
    };
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    assert_eq!(
        std::fs::metadata(&parent)?.permissions().mode() & 0o777,
        0o700
    );
    client.end().await?;
    std::fs::remove_dir_all(root)?;
    std::fs::write(std::env::var_os(CHILD).expect("child marker"), b"passed")?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_key_rejects_wrong_key_for_existing_database() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-storage-key-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut first_options = options();
    first_options.storage = StorageOptions {
        location: explicit_location(&path),
        encryption_key: Some(vec![7; 32]),
        ..Default::default()
    };
    let first = Client::create(signer.clone(), first_options.clone()).await?;
    assert!(
        first.options().storage.encryption_key.is_none(),
        "options exposed the database key"
    );
    first.end().await?;
    let second = Client::create(
        signer,
        ClientOptions {
            storage: StorageOptions {
                encryption_key: Some(vec![8; 32]),
                ..first_options.storage
            },
            ..first_options
        },
    )
    .await;
    assert!(second.is_err(), "a different database key must fail");
    std::fs::remove_file(path)?;
}

// The key length check runs before the SDK opens a database, so an in-memory
// store and no backend are enough.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn storage_key_of_wrong_length_is_invalid_input() {
    for key in [vec![0xA5; 31], vec![0xA5; 33], Vec::new()] {
        let storage = StorageOptions {
            location: crate::StorageLocation::InMemory,
            encryption_key: Some(key.clone()),
            ..Default::default()
        };
        let result = crate::client::open_store(&storage, None).await;
        match result {
            Err(XmtpError::InvalidInput(details)) => {
                assert_eq!(details.code, "InvalidInput");
                assert!(matches!(details.category, crate::ErrorCategory::Input));
                assert!(!details.retryable);
                assert_eq!(details.message, "storage encryption key must be 32 bytes");
            }
            Err(error) => panic!(
                "{}-byte key: expected invalid input, got {error}",
                key.len()
            ),
            Ok(_) => panic!("{}-byte key opened a database", key.len()),
        }
    }
}

// Moved from the Kotlin conformance program (RetainedOptions.kt): it was the
// only check of the pool range rule and of the pool values that options() returns.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn storage_pool_options_round_trip_and_reject_an_inverted_range() {
    use crate::client::StoragePoolOptions;
    let pools = [
        None,
        Some(StoragePoolOptions::default()),
        Some(StoragePoolOptions {
            min: Some(2),
            max: Some(10),
        }),
        Some(StoragePoolOptions {
            min: None,
            max: Some(7),
        }),
    ];
    for pool in pools {
        let mut configured = options();
        configured.storage.pool = pool.clone();
        let client = Client::create(crate::generate_local_signer().await, configured).await?;
        let reported = client.options().storage.pool;
        assert_eq!(
            reported.as_ref().map(|pool| (pool.min, pool.max)),
            pool.as_ref().map(|pool| (pool.min, pool.max)),
            "options() changed the pool values"
        );
        client.end().await?;
    }
    let mut inverted = options();
    inverted.storage.pool = Some(StoragePoolOptions {
        min: Some(4),
        max: Some(2),
    });
    let error = Client::create(crate::generate_local_signer().await, inverted)
        .await
        .err()
        .expect("a pool minimum above its maximum must fail");
    let XmtpError::InvalidInput(details) = error else {
        panic!("expected InvalidInput, got {error:?}");
    };
    assert_eq!(details.code, "InvalidInput");
    assert!(!details.retryable);
}

// Moved from the Kotlin conformance program (RetainedStorage.kt): a live
// verifies: STORE-010
// reconnect keeps the open store and its history; an ended client cannot reconnect.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn storage_reconnect_keeps_history_and_fails_after_end() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-storage-reconnect-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut configured = options();
    configured.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, configured).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group
        .send_text("history before reconnect".into(), None)
        .await?;
    let storage = client.storage();
    storage.reconnect().await?;
    assert_eq!(
        storage.path().await?.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
    assert!(
        group
            .messages(None)
            .await?
            .iter()
            .any(|message| message.0.id == id),
        "a live reconnect lost stored history"
    );
    client.end().await?;
    assert!(matches!(
        storage.reconnect().await,
        Err(XmtpError::ClientClosed(_))
    ));
    let _ = std::fs::remove_file(&path);
}

// verifies: STORE-009, STORE-011
#[xmtp_common::test(unwrap_try = true)]
async fn in_memory_delete_is_invalid_input_and_keeps_client_open() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let storage = client.storage();
    assert_eq!(storage.path().await?, None);
    assert!(
        matches!(storage.delete().await, Err(XmtpError::InvalidInput(ref details))
        if details.code == "InvalidInput" && !details.retryable)
    );
    assert!(client.is_registered().await?);
    client.conversations().create_group(vec![], None).await?;
    client.end().await?;
}

/// A database written before the CBOR storage marker fails to open with the
/// typed `Storage` error, its cause code, and a message that says what to do.
/// It is not retryable and not `Unknown`.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn opening_a_database_written_before_cbor_storage_is_a_typed_storage_error() {
    use xmtp_db::diesel::{Connection, SqliteConnection, connection::SimpleConnection};

    let path = std::env::temp_dir().join(format!(
        "sdk-old-encoding-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let path_text = path.to_string_lossy().into_owned();
    let storage = StorageOptions::default();
    drop(crate::client::open_store(&storage, Some(&path_text)).await?);
    // A build before the switch stored a bincode key store row and never
    // applied the marker migration `2026-10-20-000000_cbor_storage_format`.
    let mut conn = SqliteConnection::establish(&path_text)?;
    conn.batch_execute(
        "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (x'0102', 1, x'0300000000000000aabbcc'); \
         DELETE FROM __diesel_schema_migrations WHERE version = '20261020000000';",
    )?;
    drop(conn);

    match crate::client::open_store(&storage, Some(&path_text)).await {
        Err(XmtpError::Storage(details)) => {
            assert_eq!(details.code, "StorageError::OldEncodingDatabase");
            assert!(matches!(details.category, crate::ErrorCategory::Storage));
            assert!(!details.retryable);
            assert_eq!(
                details.message,
                "This database uses an older storage encoding. Keep a backup and create a new client database."
            );
        }
        Err(error) => panic!("expected a typed storage error, got {error:?}"),
        Ok(_) => panic!("a database written before the CBOR switch opened"),
    }
    let _ = std::fs::remove_file(path);
}

/// A database that holds a live group with a 16-byte id fails to open with
/// the typed `Storage` error, its cause code, and a message that says what
/// to do. It is not retryable and not `Unknown`.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn opening_a_database_with_a_live_legacy_group_is_a_typed_storage_error() {
    use xmtp_db::{
        Store,
        group::{GroupMembershipState, StoredGroup},
    };

    let path = std::env::temp_dir().join(format!(
        "sdk-legacy-group-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let path_text = path.to_string_lossy().into_owned();
    let storage = StorageOptions::default();
    let store = crate::client::open_store(&storage, Some(&path_text)).await?;
    // A build before keyed group ids stored this group.
    StoredGroup::builder()
        .id(xmtp_proto::types::GroupId::from([1u8; 16]))
        .created_at_ns(1)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("adder")
        .build()?
        .store(&store.db())?;
    drop(store);

    match crate::client::open_store(&storage, Some(&path_text)).await {
        Err(XmtpError::Storage(details)) => {
            assert_eq!(details.code, "StorageError::LegacyGroupDatabase");
            assert!(matches!(details.category, crate::ErrorCategory::Storage));
            assert!(!details.retryable);
            assert_eq!(
                details.message,
                "This database holds groups from before keyed group ids. Keep a backup and create a new client database."
            );
        }
        Err(error) => panic!("expected a typed storage error, got {error:?}"),
        Ok(_) => panic!("a database with a live 16-byte group opened"),
    }
    let _ = std::fs::remove_file(path);
}
