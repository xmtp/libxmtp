use super::*;

use std::sync::atomic::AtomicUsize;

/// A TCP relay to the test backend that counts the connections it accepts.
/// While it refuses, it closes every connection it accepts.
struct CountingRelay {
    url: String,
    connections: Arc<AtomicUsize>,
    forward: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl CountingRelay {
    async fn start() -> Result<Self, XmtpError> {
        let backend: http::Uri = xmtp_configuration::backend_test_url()
            .parse()
            .map_err(XmtpError::unknown)?;
        let target = backend
            .authority()
            .expect("backend URL has an authority")
            .to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(XmtpError::unknown)?;
        let url = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().map_err(XmtpError::unknown)?.port()
        );
        let connections = Arc::new(AtomicUsize::new(0));
        let forward = Arc::new(AtomicBool::new(true));
        let task = tokio::spawn({
            let connections = connections.clone();
            let forward = forward.clone();
            async move {
                while let Ok((mut inbound, _)) = listener.accept().await {
                    connections.fetch_add(1, Ordering::SeqCst);
                    if !forward.load(Ordering::SeqCst) {
                        continue;
                    }
                    let target = target.clone();
                    tokio::spawn(async move {
                        if let Ok(mut outbound) = tokio::net::TcpStream::connect(target).await {
                            let _ =
                                tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                        }
                    });
                }
            }
        });
        Ok(Self {
            url,
            connections,
            forward,
            task,
        })
    }

    fn backend(&self) -> Option<BackendSource> {
        Some(BackendSource::Options {
            options: BackendOptions {
                url: self.url.clone(),
                ..Default::default()
            },
        })
    }

    /// Close every later connection and reset the count.
    fn refuse(&self) {
        self.forward.store(false, Ordering::SeqCst);
        self.connections.store(0, Ordering::SeqCst);
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

impl Drop for CountingRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn directory(root: &std::path::Path, label: Option<&str>) -> StorageOptions {
    StorageOptions {
        location: StorageLocation::Directory {
            directory: root.to_string_lossy().into_owned(),
        },
        label: label.map(str::to_owned),
        ..Default::default()
    }
}

fn is_storage_location(result: &Result<Client, XmtpError>) -> bool {
    matches!(result, Err(XmtpError::StorageLocation(details))
        if details.code == "StorageLocation"
            && matches!(details.category, crate::ErrorCategory::Storage)
            && !details.retryable)
}

#[xmtp_common::test(unwrap_try = true)]
async fn default_storage_requires_a_host_location() {
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage = StorageOptions::default();
    assert!(matches!(
        Client::create(signer.clone(), settings.clone()).await,
        Err(XmtpError::StorageLocationRequired(_))
    ));
    assert!(matches!(
        Client::build(signer::identity(signer).await?, settings, None).await,
        Err(XmtpError::StorageLocationRequired(_))
    ));
}

// verifies: STORE-009
#[xmtp_common::test(unwrap_try = true)]
async fn labelled_directory_opens_the_deployment_layout_offline_from_its_record() {
    let relay = CountingRelay::start().await?;
    let root = temp_root("layout");
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.backend = relay.backend();
    settings.storage = directory(&root, Some("phone"));
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    let identifier = online.server_configuration().identifier;
    // A short printable identifier with no character the file name table
    // removes keeps its bytes, lowercased.
    assert!(
        identifier.len() <= 190
            && !identifier.starts_with(['.', ' '])
            && !identifier.ends_with(['.', ' '])
            && identifier
                .bytes()
                .all(|byte| { (0x20..=0x7e).contains(&byte) && !b"<>:\"|?*/\\".contains(&byte) }),
        "the expected deployment directory assumes a file-safe identifier: {identifier:?}"
    );
    let deployment = format!(
        "{}-{}",
        identifier.to_ascii_lowercase(),
        hex::encode(xmtp_cryptography::hash::sha256_bytes(identifier.as_bytes()))
    );
    let expected = root
        .join("phone")
        .join(&deployment)
        .join(inbox_id.checked()?)
        .join("xmtp.db3");
    assert_eq!(
        online.storage().path().await?,
        Some(expected.to_string_lossy().into_owned())
    );
    assert!(expected.is_file());
    online.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let identity = signer::identity(signer).await?;
    let offline = Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await?;
    assert_eq!(relay.connections(), 0, "offline build sent a request");
    assert_eq!(
        offline.storage().path().await?,
        Some(expected.to_string_lossy().into_owned())
    );
    assert_eq!(offline.inbox_id(), inbox_id);
    offline.end().await?;

    // The unlabelled root has no deployment record.
    settings.storage = directory(&root, None);
    let missing = Client::build(identity, settings, Some(inbox_id)).await;
    assert!(
        is_storage_location(&missing),
        "offline first start: {:?}",
        missing.err()
    );
    assert_eq!(relay.connections(), 0, "offline first start sent a request");
    assert!(!root.join(&deployment).exists());
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn unsafe_storage_label_fails_before_any_path_or_request() {
    let relay = CountingRelay::start().await?;
    let root = temp_root("label");
    let signer = crate::generate_local_signer().await;
    for label in [".", "..", "bad/name", "bad\\name", "bad:name", "bad\0name"] {
        let mut settings = options();
        settings.backend = relay.backend();
        settings.storage = directory(&root, Some(label));
        let created = Client::create(signer.clone(), settings).await;
        assert!(is_storage_location(&created), "label {label:?}");
    }
    assert_eq!(relay.connections(), 0);
    assert!(!root.exists());
}

#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_reopens_offline_without_inbox_id() {
    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit");
    std::fs::create_dir_all(&root)?;
    let db_path = root.join("chosen.sqlite");
    let attachments_dir = root.join("files");
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.backend = relay.backend();
    settings.storage.location = StorageLocation::Explicit {
        db_path: db_path.to_string_lossy().into_owned(),
        attachments_dir: attachments_dir.to_string_lossy().into_owned(),
    };
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    assert_eq!(
        online.storage().path().await?,
        Some(db_path.to_string_lossy().into_owned())
    );
    online.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let offline = Client::build(signer::identity(signer).await?, settings, None).await?;
    assert_eq!(relay.connections(), 0, "offline reopen sent a request");
    assert_eq!(offline.inbox_id(), inbox_id);
    assert_eq!(
        offline.storage().path().await?,
        Some(db_path.to_string_lossy().into_owned())
    );
    offline.end().await?;
    assert!(db_path.is_file());
    std::fs::remove_dir_all(root)?;
}

fn is_identity_mismatch(result: &Result<Client, XmtpError>) -> bool {
    matches!(result, Err(XmtpError::IdentityMismatch(details))
        if details.code == "IdentityMismatch"
            && matches!(details.category, crate::ErrorCategory::Identity)
            && !details.retryable)
}

#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_opens_only_for_an_identity_of_its_inbox() {
    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-identity");
    std::fs::create_dir_all(&root)?;
    let db_path = root.join("chosen.sqlite");
    let mut settings = options();
    settings.backend = relay.backend();
    settings.storage.location = StorageLocation::Explicit {
        db_path: db_path.to_string_lossy().into_owned(),
        attachments_dir: root.join("files").to_string_lossy().into_owned(),
    };
    let owner = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let inbox_id = owner.inbox_id();
    // An added account belongs to the inbox, but its own inbox ID differs.
    let added_signer = crate::generate_local_signer().await;
    owner
        .unsafe_add_account(added_signer.clone(), false)
        .await?;
    let added = signer::identity(added_signer).await?;
    assert_ne!(
        added.to_core()?.inbox_id(0)?,
        inbox_id.checked()?,
        "the added account must not own the inbox ID"
    );
    // The database stores the association state with the added account.
    assert!(owner.inbox_state(true).await?.identities.len() >= 2);
    owner.end().await?;

    let stranger_signer = crate::generate_local_signer().await;
    let stranger = signer::identity(stranger_signer.clone()).await?;
    let created = Client::create(stranger_signer, settings.clone()).await;
    assert!(
        is_identity_mismatch(&created),
        "create: {:?}",
        created.err()
    );
    let built = Client::build(stranger.clone(), settings.clone(), None).await;
    assert!(is_identity_mismatch(&built), "build: {:?}", built.err());

    let online = Client::build(added.clone(), settings.clone(), None).await?;
    assert_eq!(online.inbox_id(), inbox_id);
    online.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let built = Client::build(stranger, settings.clone(), None).await;
    assert!(
        is_identity_mismatch(&built),
        "offline build: {:?}",
        built.err()
    );
    let offline = Client::build(added, settings, None).await?;
    assert_eq!(offline.inbox_id(), inbox_id);
    offline.end().await?;
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}
