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
    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-delete-events-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
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
    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-delete-running-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
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

    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-delete-retry-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
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
