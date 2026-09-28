use super::*;

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
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
    let signer = crate::generate_local_signer().await;
    let identity = signer::identity(signer).await?;
    let inbox_id = InboxId(
        identity
            .to_core()?
            .inbox_id(0)
            .map_err(XmtpError::unknown)?,
    );
    let result = Client::build(identity, settings, Some(inbox_id)).await;
    std::fs::remove_file(parent)?;

    match result {
        Err(XmtpError::Storage(details))
            if details.code == "Storage"
                && matches!(details.category, crate::ErrorCategory::Storage) => {}
        Err(error) => panic!("expected storage error, got {error}"),
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
async fn dm_create_is_idempotent_and_peer_ids_survive_lookup() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert_eq!(first.peer_inbox_id().await?, Some(bo.inbox_id()));
    let again = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert_eq!(again.id(), first.id());
    let listed = alix.conversations().list(None).await?;
    let dms = listed
        .into_iter()
        .filter_map(|conversation| match conversation {
            crate::Conversation::Dm { dm } => Some(dm),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].id(), first.id());
    assert_eq!(dms[0].peer_inbox_id().await?, Some(bo.inbox_id()));
    let groups = alix
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Group),
            ..Default::default()
        }))
        .await?;
    assert!(groups.is_empty());
    let only_dms = alix
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Dm),
            ..Default::default()
        }))
        .await?;
    assert_eq!(only_dms.len(), 1);

    let alix_summary = alix.conversations().sync_all(None).await?;
    let bo_summary = bo.conversations().sync_all(None).await?;
    assert_eq!(alix_summary.eligible, 1);
    assert_eq!(alix_summary.synced, 1);
    assert_eq!(bo_summary.eligible, 1);
    assert_eq!(bo_summary.synced, 1);
    let from_peer = bo
        .conversations()
        .get_dm_by_inbox_id(alix.inbox_id())
        .await?
        .expect("the peer DM");
    assert_eq!(from_peer.id(), first.id());
    assert_eq!(from_peer.peer_inbox_id().await?, Some(alix.inbox_id()));
    let peer_groups = bo
        .conversations()
        .list(Some(crate::ListConversationsOptions {
            kind: Some(crate::ConversationKind::Group),
            ..Default::default()
        }))
        .await?;
    assert!(peer_groups.is_empty());
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn custom_permission_set_is_converted_and_invalid_set_is_rejected() {
    use crate::{
        CreateGroupOptions, GroupPermissionMode, PermissionPolicy as Policy, PermissionPolicySet,
    };
    let policy_set = PermissionPolicySet {
        add_member: Policy::Allow,
        remove_member: Policy::Deny,
        add_admin: Policy::Admin,
        remove_admin: Policy::Admin,
        update_name: Policy::Admin,
        update_description: Policy::Allow,
        update_image: Policy::Admin,
        update_disappearing: Policy::Admin,
        update_app_data: Policy::SuperAdmin,
    };
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(
            vec![],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::Custom {
                    policy_set: policy_set.clone(),
                }),
                ..Default::default()
            }),
        )
        .await?;
    let actual = group.state().await?.permissions.policy_set;
    assert!(matches!(actual.add_member, Policy::Allow));
    assert!(matches!(actual.remove_member, Policy::Deny));
    assert!(matches!(actual.add_admin, Policy::Admin));
    assert!(matches!(actual.remove_admin, Policy::Admin));
    assert!(matches!(actual.update_name, Policy::Admin));
    assert!(matches!(actual.update_description, Policy::Allow));
    assert!(matches!(actual.update_image, Policy::Admin));
    assert!(matches!(actual.update_disappearing, Policy::Admin));
    assert!(matches!(actual.update_app_data, Policy::SuperAdmin));

    let invalid = PermissionPolicySet {
        add_admin: Policy::Allow,
        ..policy_set
    };
    assert!(matches!(
        alix.conversations()
            .create_group(
                vec![],
                Some(CreateGroupOptions {
                    permissions: Some(GroupPermissionMode::Custom {
                        policy_set: invalid
                    }),
                    ..Default::default()
                })
            )
            .await,
        Err(XmtpError::InvalidInput(_))
    ));
    alix.end().await?;
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
        location: StorageLocation::Path(path.to_string_lossy().into_owned()),
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
    assert!(matches!(
        Client::build(identity.clone(), settings.clone(), None).await,
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

// verifies: CONF-076
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
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
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
