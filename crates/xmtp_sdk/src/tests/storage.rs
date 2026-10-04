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
