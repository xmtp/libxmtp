//! Client create and build: stored identity, storage and backend
//! admission, offline builds, catch-up, and installation revocation.

use super::*;

async fn build_on_new_database_fails_identity_not_found(
    allow_offline: bool,
) -> Result<(), XmtpError> {
    use xmtp_db::{Fetch, identity::StoredIdentity};

    let signer = crate::generate_local_signer().await;
    let identity = signer::identity(signer).await?;
    let inbox_id = InboxId::try_from(
        identity
            .to_core()?
            .inbox_id(0)
            .map_err(XmtpError::unknown)?,
    )?;
    let path = std::env::temp_dir().join(format!(
        "sdk-build-new-{}-{}-{}.db3",
        allow_offline,
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    settings.allow_offline = allow_offline;
    settings.backend = Some(BackendSource::Options {
        options: BackendOptions {
            url: "http://127.0.0.1:1".into(),
            ..Default::default()
        },
    });

    let result = Client::build(identity, settings.clone(), Some(inbox_id.clone())).await;
    assert!(
        matches!(result, Err(XmtpError::IdentityNotFound(ref details))
        if details.code == "IdentityNotFound"
            && matches!(details.category, crate::ErrorCategory::Identity)
            && !details.retryable)
    );
    assert!(!path.exists(), "build created a new database");
    let store = crate::client::open_store(&settings.storage, Some(&path.to_string_lossy())).await?;
    let stored: Option<StoredIdentity> = store.db().fetch(&()).map_err(XmtpError::unknown)?;
    assert!(stored.is_none(), "build registered a new identity");
    drop(store);
    std::fs::remove_file(path).map_err(XmtpError::unknown)?;
    Ok(())
}

// verifies: STORE-007
#[xmtp_common::test(unwrap_try = true)]
async fn build_on_new_database_fails_identity_not_found_online() {
    build_on_new_database_fails_identity_not_found(false).await?;
}

// verifies: STORE-007
#[xmtp_common::test(unwrap_try = true)]
async fn build_on_new_database_fails_identity_not_found_offline() {
    build_on_new_database_fails_identity_not_found(true).await?;
}

// A path below a regular file fails with ENOTDIR for every user, root included.
// A mode 000 directory does not: root can still traverse it.
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn build_with_inaccessible_database_path_returns_storage_error() {
    let parent = std::env::temp_dir().join(format!(
        "sdk-build-inaccessible-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    std::fs::write(&parent, b"not a directory")?;
    let path = parent.join("client.sqlite");
    assert!(path.try_exists().is_err());
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let signer = crate::generate_local_signer().await;
    let identity = signer::identity(signer).await?;
    let inbox_id = InboxId::try_from(
        identity
            .to_core()?
            .inbox_id(0)
            .map_err(XmtpError::unknown)?,
    )?;
    let result = Client::build(identity, settings, Some(inbox_id)).await;
    std::fs::remove_file(parent)?;

    match result {
        Err(XmtpError::StorageLocation(details))
            if details.code == "StorageLocation"
                && matches!(details.category, crate::ErrorCategory::Storage)
                && !details.retryable => {}
        Err(error) => panic!("expected storage location error, got {error}"),
        Ok(_) => panic!("build opened an inaccessible database"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn revoke_all_other_installations_skips_current_installation() {
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), options()).await?;
    assert_eq!(first.inbox_state(true).await?.installations.len(), 1);
    assert!(
        first
            .unsafe_revoke_all_other_installations_signature_request()
            .await?
            .is_none()
    );

    let second = Client::create(signer.clone(), options()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = first
        .conversations()
        .create_group(vec![peer.inbox_id()], None)
        .await?;
    let solo = first.conversations().create_group(vec![], None).await?;
    second.conversations().sync().await?;
    let revoked_group = second
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("second installation group");
    let crate::Conversation::Group {
        group: revoked_group,
    } = revoked_group
    else {
        panic!("expected group")
    };
    assert_eq!(first.inbox_state(true).await?.installations.len(), 2);
    let request = first
        .unsafe_revoke_all_other_installations_signature_request()
        .await?
        .expect("the other installation needs a signature");
    request.sign(signer).await?;
    first.unsafe_apply_signature_request(request).await?;
    let state = first.inbox_state(true).await?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(state.installations[0].id, first.installation_id());
    group.sync().await?;
    revoked_group.sync().await?;
    assert!(
        revoked_group
            .update_name("revoked change".into())
            .await
            .is_err()
    );
    group.update_name("after revoke".into()).await?;
    group.sync().await?;
    assert_eq!(group.state().await?.name, "after revoke");
    peer.conversations().sync().await?;
    let peer_group = peer
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("peer group");
    let crate::Conversation::Group { group: peer_group } = peer_group else {
        panic!("expected group")
    };
    peer_group.sync().await?;
    solo.update_name("solo after revoke".into()).await?;
    assert_eq!(solo.state().await?.name, "solo after revoke");
    first.end().await?;
    second.end().await?;
    peer.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_replays_once_and_preserves_bounded_progress() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = bo
        .conversations()
        .create_group(vec![alix.inbox_id()], None)
        .await?;
    let expected = (0..5).map(|i| format!("owed {i}")).collect::<Vec<_>>();
    for text in &expected {
        group.send_text(text.clone(), None).await?;
    }

    match alix.catch_up_to_live(Some(1)).await {
        Ok(bounded) => {
            assert!(bounded.completed);
            assert!(bounded.messages <= 5);
        }
        Err(XmtpError::Unknown(details)) => {
            assert!(
                details.message.starts_with("Catch-up did not complete:"),
                "expected a catch-up failure: {}",
                details.message
            );
        }
        Err(error) => panic!("expected a catch-up failure: {error}"),
    }
    let full = alix.catch_up_to_live(None).await?;
    assert!(full.completed);
    let received = alix
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("catch-up joined the group");
    let crate::Conversation::Group { group: received } = received else {
        panic!("expected a group");
    };
    let actual = received
        .messages(None)
        .await?
        .into_iter()
        .filter_map(|message| match message.0.content {
            MessageContent::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let again = alix.catch_up_to_live(None).await?;
    assert!(again.completed);
    assert_eq!((again.conversations, again.messages), (0, 0));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn backend_url_is_required_and_offline_choice_is_explicit() {
    use xmtp_db::prelude::QueryServerConfiguration;

    assert!(
        crate::Backend::connect(BackendOptions::default())
            .await
            .is_err()
    );
    let unreachable = BackendOptions {
        url: "http://127.0.0.1:1".into(),
        ..Default::default()
    };
    let backend = Arc::new(crate::Backend::connect(unreachable.clone()).await?);
    assert_eq!(backend.options.url, unreachable.url);
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-offline-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage = StorageOptions {
        location: explicit_location(&path),
        ..Default::default()
    };
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    let group_id = online
        .conversations()
        .create_group(vec![], None)
        .await?
        .id();
    let stored = online
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("first configuration");
    let first_fetch = stored.fetched_at_ns;
    online.inner.context.db().store_server_configuration(
        &stored.identifier,
        "http://127.0.0.1:2",
        &stored.response,
        first_fetch,
    )?;
    online.end().await?;
    let identity = signer::identity(signer).await?;
    settings.backend = Some(BackendSource::Connected {
        backend: backend.clone(),
    });
    assert!(!settings.allow_offline);
    settings.allow_offline = false;
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await,
        Err(XmtpError::ConfigurationUnavailable(_))
    ));
    settings.allow_offline = ClientOptions::default().allow_offline;
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await,
        Err(XmtpError::ConfigurationUnavailable(_))
    ));
    settings.backend = options().backend;
    let client = Client::build(identity.clone(), settings.clone(), Some(inbox_id.clone())).await?;
    let second_fetch = client
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("refetched configuration")
        .fetched_at_ns;
    assert!(
        second_fetch > first_fetch,
        "default build did not fetch configuration"
    );
    assert_eq!(client.inbox_id(), inbox_id);
    assert!(
        client
            .conversations()
            .list(None)
            .await?
            .iter()
            .any(|conversation| {
                match conversation {
                    crate::Conversation::Group { group } => group.id() == group_id,
                    crate::Conversation::Dm { .. } => false,
                }
            })
    );
    client.end().await?;
    settings.backend = Some(BackendSource::Connected { backend });
    settings.allow_offline = true;
    let mut directory = settings.clone();
    directory.storage.location = StorageLocation::Directory {
        directory: path.with_extension("root").to_string_lossy().into_owned(),
    };
    assert!(matches!(
        Client::build(identity.clone(), directory, None).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let explicit = Client::build(identity, settings, Some(inbox_id.clone())).await?;
    assert_eq!(explicit.inbox_id(), inbox_id);
    assert!(
        explicit
            .conversations()
            .get_by_id(group_id)
            .await?
            .is_some()
    );
    explicit.end().await?;
    std::fs::remove_file(path)?;
}

// verifies: CONF-034, CONF-076
#[xmtp_common::test(unwrap_try = true)]
async fn offline_build_with_moved_url_uses_stored_copy() {
    use xmtp_db::prelude::QueryServerConfiguration;

    let path = std::env::temp_dir().join(format!(
        "sdk-moved-backend-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    let stored = online
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("stored configuration");
    online.end().await?;

    settings.allow_offline = true;
    settings.backend = Some(BackendSource::Options {
        options: BackendOptions {
            url: "http://127.0.0.1:1".into(),
            ..Default::default()
        },
    });
    let identity = signer::identity(signer).await?;
    assert_ne!(stored.backend_url, "http://127.0.0.1:1");
    let offline = Client::build(identity, settings, Some(inbox_id)).await?;
    assert_eq!(offline.server_configuration().identifier, stored.identifier);
    let retained = offline
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("stored configuration after offline build");
    assert_eq!(retained.backend_url, stored.backend_url);
    assert_eq!(retained.fetched_at_ns, stored.fetched_at_ns);
    offline.end().await?;
    std::fs::remove_file(path)?;
}

/// A real offline build whose database is bound to another deployment: the
/// first request re-checks the backend, and the app gets the
/// check's own code.
// verifies: CONF-064, CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn offline_build_on_another_deployment_fails_its_first_request_with_backend_mismatch() {
    use xmtp_db::prelude::QueryServerConfiguration;

    let path = std::env::temp_dir().join(format!(
        "sdk-other-deployment-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = StorageLocation::Explicit {
        db_path: path.to_string_lossy().into_owned(),
        attachments_dir: path
            .with_extension("attachments")
            .to_string_lossy()
            .into_owned(),
    };
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    bind_other_deployment(&online)?;
    online.end().await?;

    settings.allow_offline = true;
    let identity = signer::identity(signer).await?;
    let offline = Client::build(identity, settings, Some(inbox_id)).await?;
    // A direct network read is the first request.
    let error = offline.inbox_state(true).await.unwrap_err();
    let XmtpError::BackendMismatch(details) = error else {
        panic!("expected BackendMismatch, got {error:?}");
    };
    assert_eq!(details.code, "BackendMismatch");
    assert!(!details.retryable);
    offline.end().await?;
    std::fs::remove_file(path)?;

    /// Store the configuration of another deployment at another URL, so the
    /// next offline build must re-check before its first request.
    fn bind_other_deployment(client: &Client) -> Result<(), XmtpError> {
        let db = client.inner.context.db();
        let stored = db
            .server_configuration()
            .map_err(XmtpError::from_core)?
            .expect("stored configuration");
        db.store_server_configuration(
            "org.example.other-deployment",
            "http://moved.example",
            &stored.response,
            stored.fetched_at_ns,
        )
        .map_err(XmtpError::from_core)
    }
}

/// After a blocked connection, a sync and later calls keep the typed
/// configuration code; only end() makes the client `ClientClosed`.
// verifies: CONF-064, CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn a_blocked_connection_keeps_its_code_on_sync_and_later_calls() {
    use xmtp_db::prelude::QueryServerConfiguration;

    let path = std::env::temp_dir().join(format!(
        "sdk-blocked-sync-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = StorageLocation::Explicit {
        db_path: path.to_string_lossy().into_owned(),
        attachments_dir: path
            .with_extension("attachments")
            .to_string_lossy()
            .into_owned(),
    };
    let online = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = online.inbox_id();
    {
        let db = online.inner.context.db();
        let stored = db.server_configuration()?.expect("stored configuration");
        db.store_server_configuration(
            "org.example.other-deployment",
            "http://moved.example",
            &stored.response,
            stored.fetched_at_ns,
        )?;
    }
    online.end().await?;

    settings.allow_offline = true;
    let identity = signer::identity(signer).await?;
    let offline = Client::build(identity, settings, Some(inbox_id)).await?;
    let first = offline.conversations().sync_all(None).await.unwrap_err();
    assert!(
        matches!(first, XmtpError::BackendMismatch(_)),
        "sync_all: {first:?}"
    );
    let later = offline
        .conversations()
        .list(None)
        .await
        .err()
        .expect("a later call failed");
    assert!(
        matches!(later, XmtpError::BackendMismatch(_)),
        "a later call: {later:?}"
    );
    offline.end().await?;
    let ended = offline.conversations().sync_all(None).await.unwrap_err();
    assert!(
        matches!(ended, XmtpError::ClientClosed(_)),
        "after end: {ended:?}"
    );
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_create_keeps_one_inbox_id() {
    let signer = crate::generate_local_signer().await;
    let results =
        futures::future::join_all((0..6).map(|_| Client::create(signer.clone(), options()))).await;
    let winners = results.into_iter().flatten().collect::<Vec<_>>();
    assert!(!winners.is_empty());
    let inbox_id = winners[0].inbox_id();
    for client in &winners {
        assert_eq!(client.inbox_id(), inbox_id);
    }
    let state = winners[0].inbox_state(true).await?;
    assert_eq!(state.inbox_id, inbox_id);
    assert_eq!(state.identities.len(), 1);
    assert_eq!(state.installations.len(), winners.len());
    for client in winners {
        client.end().await?;
    }
}
