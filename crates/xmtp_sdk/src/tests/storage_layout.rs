use super::*;

use std::sync::atomic::AtomicUsize;

// Identity fixtures need no pool tasks. Synchronous connection setup keeps
// fixture removal separate from r2d2's background connection replenishment.
fn layout_options() -> ClientOptions {
    let mut settings = options();
    settings.storage.single_connection = true;
    settings
}

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
        single_connection: true,
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
    let mut settings = layout_options();
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
    let mut settings = layout_options();
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
    let stranger = signer::identity(crate::generate_local_signer().await).await?;
    let denied = Client::build(stranger, settings.clone(), Some(inbox_id.clone())).await;
    if let Ok(client) = &denied {
        client.end().await?;
    }
    assert!(
        is_identity_mismatch(&denied),
        "offline supplied inbox: {:?}",
        denied.err()
    );
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
        let mut settings = layout_options();
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
    let mut settings = layout_options();
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

fn explicit(root: &std::path::Path) -> StorageLocation {
    StorageLocation::Explicit {
        db_path: root.join("chosen.sqlite").to_string_lossy().into_owned(),
        attachments_dir: root.join("files").to_string_lossy().into_owned(),
    }
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
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
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
    let supplied = Client::build(stranger.clone(), settings.clone(), Some(inbox_id.clone())).await;
    if let Ok(client) = &supplied {
        client.end().await?;
    }
    assert!(
        is_identity_mismatch(&supplied),
        "supplied inbox: {:?}",
        supplied.err()
    );

    let online = Client::build(added.clone(), settings.clone(), None).await?;
    assert_eq!(online.inbox_id(), inbox_id);
    online.end().await?;
    let online_supplied =
        Client::build(added.clone(), settings.clone(), Some(inbox_id.clone())).await?;
    assert_eq!(online_supplied.inbox_id(), inbox_id);
    online_supplied.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let built = Client::build(stranger.clone(), settings.clone(), None).await;
    assert!(
        is_identity_mismatch(&built),
        "offline build: {:?}",
        built.err()
    );
    let supplied = Client::build(stranger, settings.clone(), Some(inbox_id.clone())).await;
    if let Ok(client) = &supplied {
        client.end().await?;
    }
    assert!(
        is_identity_mismatch(&supplied),
        "offline supplied inbox: {:?}",
        supplied.err()
    );
    let offline = Client::build(added.clone(), settings.clone(), None).await?;
    assert_eq!(offline.inbox_id(), inbox_id);
    offline.end().await?;
    let offline_supplied = Client::build(added, settings, Some(inbox_id.clone())).await?;
    assert_eq!(offline_supplied.inbox_id(), inbox_id);
    offline_supplied.end().await?;
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}

// The build sweeps expired pending uploads, then seeds the key package tasks
// when it registers its workers, just before it starts them. A rejected
// identity must not reach either point, since both act as the database's
// installation.
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_rejects_an_identity_before_the_build_prepares_workers() {
    use xmtp_db::{ConnectionExt, attachments::QueryPendingAttachment as _, diesel::prelude::*};

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-identity-workers");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    settings.attachments = Some(crate::AttachmentOptions {
        allow_private_network: true,
        ..Default::default()
    });
    let owner = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let digest = owner
        .attachments()
        .create(crate::AttachmentSource::Bytes {
            bytes: b"owned".to_vec(),
            filename: Some("note.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?
        .remote_attachment()
        .content_digest;
    owner.end().await?;
    let db_path = root.join("chosen.sqlite");
    let staged = root.join("files").join(".staged").join(&digest);
    assert!(staged.is_file());
    let tasks = async || -> Result<i64, XmtpError> {
        let (store, _) = crate::client::open_store_if_present(&settings.storage, &db_path)
            .await?
            .expect("the database stays");
        let count = store
            .db()
            .raw_query(|conn| xmtp_db::schema::tasks::table.count().get_result(conn))?;
        store.db().disconnect()?;
        Ok(count)
    };
    {
        let (store, _) = crate::client::open_store_if_present(&settings.storage, &db_path)
            .await?
            .expect("the database stays");
        store.db().raw_query(|conn| {
            xmtp_db::diesel::delete(xmtp_db::schema::tasks::table).execute(conn)
        })?;
        store.db().disconnect()?;
    }
    assert_eq!(tasks().await?, 0);

    // Every pending upload has expired for the stranger's options.
    let mut stranger_settings = settings.clone();
    stranger_settings.attachments = Some(crate::AttachmentOptions {
        max_pending_age_seconds: Some(0),
        allow_private_network: true,
        ..Default::default()
    });
    let stranger = signer::identity(crate::generate_local_signer().await).await?;
    let built = Client::build(stranger, stranger_settings, None).await;
    assert!(is_identity_mismatch(&built), "build: {:?}", built.err());
    let (store, _) = crate::client::open_store_if_present(&settings.storage, &db_path)
        .await?
        .expect("the database stays");
    assert!(
        store
            .db()
            .get_pending_attachment(&digest)
            .map_err(XmtpError::unknown)?
            .is_some(),
        "the rejected build swept the pending upload"
    );
    store.db().disconnect()?;
    drop(store);
    assert!(
        staged.is_file(),
        "the rejected build deleted the staged file"
    );
    assert_eq!(tasks().await?, 0, "the rejected build prepared its workers");
    std::fs::remove_dir_all(root)?;
}

// With no live inbox for the identifier, the build falls back to the inbox the
// identifier created, whose directory holds the stored identity.
#[xmtp_common::test(unwrap_try = true)]
async fn directory_storage_rejects_its_removed_creator() {
    let root = temp_root("directory-removed");
    let mut settings = layout_options();
    settings.storage = directory(&root, None);
    let creator_signer = crate::generate_local_signer().await;
    let creator = signer::identity(creator_signer.clone()).await?;
    let client = Client::create(creator_signer.clone(), settings.clone()).await?;
    let database = client.storage().path().await?.expect("file database");
    let recovery_signer = crate::generate_local_signer().await;
    let recovery = signer::identity(recovery_signer.clone()).await?;
    client
        .unsafe_add_account(recovery_signer.clone(), false)
        .await?;
    client
        .change_recovery_identifier(creator_signer.clone(), recovery)
        .await?;
    client
        .remove_account(recovery_signer, creator.clone())
        .await?;
    assert_eq!(
        creator.to_core()?.inbox_id(0)?,
        client.inbox_id().checked()?
    );
    client.end().await?;

    let built = Client::build(creator, settings.clone(), None).await;
    assert!(is_identity_mismatch(&built), "build: {:?}", built.err());
    let created = Client::create(creator_signer, settings).await;
    assert!(
        is_identity_mismatch(&created),
        "create: {:?}",
        created.err()
    );
    assert!(std::path::Path::new(&database).is_file());
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_fetches_a_removal_made_on_another_installation() {
    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-removed-elsewhere");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root.join("creator"));
    let creator_signer = crate::generate_local_signer().await;
    let creator = signer::identity(creator_signer.clone()).await?;
    let client = Client::create(creator_signer.clone(), settings.clone()).await?;
    let recovery_signer = crate::generate_local_signer().await;
    let recovery = signer::identity(recovery_signer.clone()).await?;
    client
        .unsafe_add_account(recovery_signer.clone(), false)
        .await?;
    client
        .change_recovery_identifier(creator_signer, recovery)
        .await?;
    assert_eq!(
        creator.to_core()?.inbox_id(0)?,
        client.inbox_id().checked()?
    );
    client.end().await?;

    // Another installation removes the creator, so only the network holds
    // the removal.
    let mut elsewhere = settings.clone();
    elsewhere.storage.location = explicit(&root.join("recovery"));
    let other = Client::create(recovery_signer.clone(), elsewhere).await?;
    other
        .remove_account(recovery_signer, creator.clone())
        .await?;
    let state = other.inbox_state(true).await?;
    assert!(
        state
            .identities
            .iter()
            .all(|identity| identity.identifier != creator.identifier),
        "the creator is still a member"
    );
    other.end().await?;

    let online = Client::build(creator.clone(), settings.clone(), None).await;
    assert!(
        is_identity_mismatch(&online),
        "online build: {:?}",
        online.err()
    );

    // The online check stored the removal it fetched.
    relay.refuse();
    settings.allow_offline = true;
    let offline = Client::build(creator, settings, None).await;
    assert!(
        is_identity_mismatch(&offline),
        "offline build: {:?}",
        offline.err()
    );
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}

// verifies: CONF-033
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_sends_no_identity_request_to_a_deployment_it_refuses() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, prelude::QueryServerConfiguration};

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-refused-deployment");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    let creator_signer = crate::generate_local_signer().await;
    let client = Client::create(creator_signer.clone(), settings.clone()).await?;
    // The database is bound to another deployment and holds no identity
    // update, so a fetch of the inbox's identity updates would store them.
    let db = client.inner.context.db();
    let stored = db
        .server_configuration()?
        .expect("create stores the configuration");
    db.store_server_configuration(
        "another-deployment",
        &stored.backend_url,
        &stored.response,
        stored.fetched_at_ns,
    )?;
    db.raw_query(|conn| {
        xmtp_db::diesel::delete(xmtp_db::schema::identity_updates::table).execute(conn)
    })?;
    client.end().await?;

    // The backend URL moved, so the build asks the backend for its
    // deployment before any other request.
    settings.backend = options().backend;
    let built = Client::build(
        signer::identity(creator_signer).await?,
        settings.clone(),
        None,
    )
    .await;
    assert!(
        matches!(built, Err(XmtpError::BackendMismatch(_))),
        "build: {:?}",
        built.err()
    );

    let db_path = root.join("chosen.sqlite");
    let (store, _) = crate::client::open_store_if_present(&settings.storage, &db_path)
        .await?
        .expect("the database stays");
    let updates: i64 = store.db().raw_query(|conn| {
        xmtp_db::schema::identity_updates::table
            .count()
            .get_result(conn)
    })?;
    assert_eq!(updates, 0, "the refused deployment got an identity request");
    store.db().disconnect()?;
    std::fs::remove_dir_all(root)?;
}

// verifies: CONF-072
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_without_identity_sends_no_request_after_a_recorded_conflict() {
    use xmtp_db::prelude::QueryServerConfiguration;

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-recorded-conflict");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    // The database holds no identity, so create does not know the inbox.
    let db_path = root.join("chosen.sqlite");
    let store =
        crate::client::open_store(&settings.storage, Some(&db_path.to_string_lossy())).await?;
    store
        .db()
        .store_server_configuration("stored-deployment", &relay.url, b"", 0)?;
    store
        .db()
        .record_server_configuration_conflict("another-deployment")?;
    drop(store);

    let created = Client::create(crate::generate_local_signer().await, settings).await;
    assert!(
        matches!(created, Err(XmtpError::BackendMismatch(_))),
        "create: {:?}",
        created.err()
    );
    assert_eq!(relay.connections(), 0, "create sent a request");
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_without_identity_updates_opens_offline_only_for_its_creator() {
    use xmtp_db::{ConnectionExt, diesel::RunQueryDsl};

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-no-updates");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    let creator_signer = crate::generate_local_signer().await;
    let client = Client::create(creator_signer.clone(), settings.clone()).await?;
    let inbox_id = client.inbox_id();
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::delete(xmtp_db::schema::identity_updates::table).execute(conn)
    })?;
    client.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let stranger_signer = crate::generate_local_signer().await;
    let built = Client::build(
        signer::identity(stranger_signer.clone()).await?,
        settings.clone(),
        None,
    )
    .await;
    assert!(is_identity_mismatch(&built), "build: {:?}", built.err());
    let created = Client::create(stranger_signer, settings.clone()).await;
    assert!(
        is_identity_mismatch(&created),
        "create: {:?}",
        created.err()
    );
    let reopened = Client::build(signer::identity(creator_signer).await?, settings, None).await?;
    assert_eq!(reopened.inbox_id(), inbox_id);
    reopened.end().await?;
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}

// Offline, the state of a history with a smart contract wallet signature
// that the database has not cached cannot be computed. Membership is then
// unknown, so not even the creator opens the database.
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_refuses_its_creator_offline_when_membership_needs_a_wallet_check() {
    use xmtp_db::{identity_update::StoredIdentityUpdate, prelude::QueryIdentityUpdates};
    use xmtp_id::associations::{
        AccountId, MemberIdentifier,
        builder::SignatureRequestBuilder,
        test_utils::MockSmartContractSignatureVerifier,
        unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
    };

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-wallet-check");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    let creator_signer = crate::generate_local_signer().await;
    let creator = signer::identity(creator_signer.clone()).await?;
    let client = Client::create(creator_signer.clone(), settings.clone()).await?;
    let inbox_id = client.inner.inbox_id().to_owned();
    let supplied_id = client.inbox_id();

    // The database alone holds an update in which the creator adds a smart
    // contract wallet. The mock verifier accepts the wallet's signature here;
    // offline, nothing can check it.
    let wallet = signer::identity(crate::generate_local_signer().await).await?;
    let wallet_member = MemberIdentifier::from(wallet.to_core()?);
    let mut request = SignatureRequestBuilder::new(&inbox_id)
        .add_association(wallet_member, creator.to_core()?.into())
        .build();
    let text = request.signature_text();
    let Signature::Ecdsa(bytes) = signer::sign(creator_signer, SigningRequest { text }).await?
    else {
        panic!("the local signer signs with ECDSA");
    };
    let verifier = MockSmartContractSignatureVerifier::new(true);
    request
        .add_signature(UnverifiedSignature::new_recoverable_ecdsa(bytes), &verifier)
        .await?;
    request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                vec![1; 65],
                AccountId::new_evm(1, wallet.identifier.clone()),
                Some(1),
            ),
            &verifier,
        )
        .await?;
    let db = client.inner.context.db();
    let last = db.get_identity_updates(&inbox_id, None, None)?;
    let sequence_id = last.last().expect("the creator's updates").sequence_id + 1;
    db.insert_or_ignore_identity_updates(&[StoredIdentityUpdate::new(
        inbox_id,
        sequence_id,
        0,
        request.build_identity_update()?.into(),
    )])?;
    client.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let offline = Client::build(creator.clone(), settings.clone(), None).await;
    assert!(
        is_identity_mismatch(&offline),
        "offline build: {:?}",
        offline.err()
    );
    let supplied = Client::build(creator, settings, Some(supplied_id)).await;
    if let Ok(client) = &supplied {
        client.end().await?;
    }
    assert!(
        is_identity_mismatch(&supplied),
        "offline supplied inbox: {:?}",
        supplied.err()
    );
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}

// An update the database cannot read is a broken store, not a verdict on the
// identity, so the offline build reports it as it is.
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_storage_reports_an_unreadable_identity_update_offline() {
    use xmtp_db::{identity_update::StoredIdentityUpdate, prelude::QueryIdentityUpdates};

    let relay = CountingRelay::start().await?;
    let root = temp_root("explicit-unreadable-update");
    std::fs::create_dir_all(&root)?;
    let mut settings = layout_options();
    settings.backend = relay.backend();
    settings.storage.location = explicit(&root);
    let creator_signer = crate::generate_local_signer().await;
    let creator = signer::identity(creator_signer.clone()).await?;
    let client = Client::create(creator_signer, settings.clone()).await?;
    let inbox_id = client.inner.inbox_id().to_owned();
    client.end().await?;

    relay.refuse();
    settings.allow_offline = true;
    let valid = Client::build(creator.clone(), settings.clone(), None).await?;
    assert_eq!(valid.inner.inbox_id(), inbox_id);
    let db = valid.inner.context.db();
    let last = db.get_identity_updates(&inbox_id, None, None)?;
    let sequence_id = last.last().expect("the creator's updates").sequence_id + 1;
    db.insert_or_ignore_identity_updates(&[StoredIdentityUpdate::new(
        inbox_id,
        sequence_id,
        0,
        vec![0xff; 8],
    )])?;
    valid.end().await?;

    let offline = Client::build(creator, settings, None).await;
    let details = match offline {
        Err(XmtpError::Unknown(details)) => details,
        other => panic!("offline build: {:?}", other.err()),
    };
    assert_eq!(details.code, "Unknown");
    assert!(matches!(
        details.category,
        crate::error::ErrorCategory::Unknown
    ));
    assert!(!details.retryable);
    assert!(
        details
            .message
            .starts_with("Association error: decoding proto"),
        "offline build cause: {}",
        details.message
    );
    assert_eq!(relay.connections(), 0, "offline check sent a request");
    std::fs::remove_dir_all(root)?;
}
