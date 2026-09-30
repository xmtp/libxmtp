use super::*;

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
    settings.storage.location = StorageLocation::Path(relative.to_string_lossy().into_owned());
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
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
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
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
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
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
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

// verifies: STORE-009
#[xmtp_common::test(unwrap_try = true)]
async fn storage_default_requires_host_and_directory_names_are_unique() {
    let default = StorageOptions::default();
    assert!(matches!(
        native_storage_path(&default, "inbox-a"),
        Err(XmtpError::StorageLocationRequired(_))
    ));
    let built = Client::build(
        PublicIdentity {
            identifier: "invalid".into(),
            kind: PublicIdentityKind::Ethereum,
        },
        ClientOptions::default(),
        None,
    )
    .await;
    assert!(matches!(built, Err(XmtpError::StorageLocationRequired(_))));

    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-storage-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let options = StorageOptions {
        location: StorageLocation::Directory(directory.to_string_lossy().into_owned()),
        label: None,
        encryption_key: None,
        pool: None,
        single_connection: false,
    };
    let first_path = native_storage_path(&options, "inbox-a")?.expect("directory path");
    let second_path = native_storage_path(&options, "inbox-b")?.expect("directory path");
    assert_ne!(first_path, second_path);
    assert!(first_path.ends_with("xmtp-inbox-a.db3"));
    assert!(second_path.ends_with("xmtp-inbox-b.db3"));
    let first_store = crate::client::open_store(&options, "inbox-a").await?;
    let second_store = crate::client::open_store(&options, "inbox-b").await?;
    assert!(std::path::Path::new(&first_path).exists());
    assert!(std::path::Path::new(&second_path).exists());
    let labeled = StorageOptions {
        label: Some("phone".into()),
        ..options.clone()
    };
    let labeled_path = native_storage_path(&labeled, "inbox-a")?.expect("directory path");
    assert!(labeled_path.ends_with("xmtp-phone-inbox-a.db3"));
    assert_ne!(first_path, labeled_path);
    let exact_path = directory
        .join("chosen.sqlite")
        .to_string_lossy()
        .into_owned();
    let path_options = StorageOptions {
        location: StorageLocation::Path(exact_path.clone()),
        ..options.clone()
    };
    assert_eq!(
        native_storage_path(&path_options, "inbox-a")?.expect("exact path"),
        exact_path
    );
    let mut file_options = self::options();
    file_options.storage = path_options;
    let file_client = Client::create(crate::generate_local_signer().await, file_options).await?;
    assert_eq!(
        file_client.storage().path().await?,
        Some(exact_path.clone())
    );
    assert!(std::path::Path::new(&exact_path).is_file());
    file_client.end().await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&directory)?.permissions().mode() & 0o777,
            0o700
        );
    }
    drop(first_store);
    drop(second_store);
    std::fs::remove_dir_all(directory)?;
}

#[xmtp_common::test(unwrap_try = true)]
fn storage_label_rejects_unsafe_characters() {
    for label in ["bad/name", "bad\\name", "bad:name", "bad\0name"] {
        let options = StorageOptions {
            location: StorageLocation::Directory(std::env::temp_dir().to_string_lossy().into()),
            label: Some(label.into()),
            ..Default::default()
        };
        assert!(matches!(
            native_storage_path(&options, "inbox-a"),
            Err(XmtpError::InvalidInput(_))
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn wasm_directory_reports_the_store_path() {
    let options = StorageOptions {
        location: StorageLocation::Directory("sdk-files".into()),
        label: Some("phone".into()),
        ..Default::default()
    };
    let reported = crate::client::wasm_storage_path(&options, "inbox-a")?.expect("file path");
    let location = crate::client::wasm_store_location(&options, "inbox-a")?;
    let xmtp_db::StorageOption::Persistent(opened) = &location else {
        panic!("Directory storage must be persistent");
    };
    assert_eq!(reported, opened.as_str());
    assert_eq!(reported, "sdk-files/xmtp-phone-inbox-a.db3");
}
