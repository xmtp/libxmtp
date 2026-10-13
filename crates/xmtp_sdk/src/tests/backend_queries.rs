use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn backend_only_identity_and_message_queries() {
    let path = std::env::temp_dir().join(format!(
        "sdk-backend-queries-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let Some(BackendSource::Options {
        options: backend_options,
    }) = options().backend
    else {
        panic!("test uses backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options).await?);
    let source = BackendSource::Connected { backend };
    let identity = client.identity();
    let inbox =
        crate::static_helpers::inbox_id_for_with_backend(source.clone(), identity.clone()).await?;
    assert_eq!(inbox, client.inbox_id());
    let availability =
        crate::static_helpers::can_message_with_backend(source.clone(), vec![identity.clone()])
            .await?;
    assert!(availability[&format!("ethereum:{}", identity.identifier)]);
    let states =
        crate::static_helpers::inbox_states_with_backend(source.clone(), vec![inbox.clone()])
            .await?;
    assert_eq!(states[0].inbox_id, inbox);
    assert!(
        crate::static_helpers::is_address_authorized_with_backend(
            source.clone(),
            inbox.clone(),
            identity.identifier,
        )
        .await?
    );
    assert!(
        crate::static_helpers::is_installation_authorized_with_backend(
            source.clone(),
            inbox,
            client.installation_id(),
        )
        .await?
    );
    let group = client.conversations().create_group(vec![], None).await?;
    group.send_text("metadata".into(), None).await?;
    let metadata = crate::static_helpers::newest_message_metadata_with_backend(
        source.clone(),
        vec![group.id()],
    )
    .await?;
    assert_eq!(metadata.len(), 1);
    let group_id = group.id().into_checked()?;
    assert!(metadata[&group_id].created_at.0 > 0);
    let first_metadata = metadata[&group_id].created_at.0;
    let first_sequence_id = metadata[&group_id].sequence_id;
    group.send_text("newer metadata".into(), None).await?;
    let updated_metadata = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let metadata = crate::static_helpers::newest_message_metadata_with_backend(
                source.clone(),
                vec![group.id()],
            )
            .await?;
            if metadata
                .get(&group_id)
                .is_some_and(|entry| entry.sequence_id > first_sequence_id)
            {
                return Ok::<_, XmtpError>(metadata);
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("new message metadata was not visible")?;
    assert!(updated_metadata[&group_id].created_at.0 >= first_metadata);
    assert!(updated_metadata[&group_id].sequence_id > first_sequence_id);
    let connected_client = Client::build(
        client.identity(),
        ClientOptions {
            backend: Some(source),
            ..settings
        },
        Some(client.inbox_id()),
    )
    .await?;
    assert_eq!(connected_client.inbox_id(), client.inbox_id());
    connected_client.end().await?;
    client.end().await?;
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn can_message_keeps_kinds_for_static_and_instance_queries() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let Some(BackendSource::Options {
        options: backend_options,
    }) = options().backend
    else {
        panic!("test uses backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options.clone()).await?);
    let same_text = "1111111111111111111111111111111111111111";
    let ethereum = PublicIdentity {
        identifier: same_text.into(),
        kind: PublicIdentityKind::Ethereum,
    };
    let passkey = PublicIdentity {
        identifier: same_text.into(),
        kind: PublicIdentityKind::Passkey,
    };
    let registered = client.identity();
    let identities = vec![ethereum, passkey, registered.clone()];
    let expected_registered = format!("ethereum:{}", registered.identifier);
    let verify = |result: std::collections::HashMap<String, bool>| {
        assert_eq!(result.len(), 3);
        assert!(!result["ethereum:1111111111111111111111111111111111111111"]);
        assert!(!result["passkey:1111111111111111111111111111111111111111"]);
        assert!(result[&expected_registered]);
    };
    verify(client.can_message(identities.clone()).await?);
    assert_eq!(client.inbox_id_for(identities[0].clone()).await?, None);
    assert_eq!(
        client.inbox_id_for(registered.clone()).await?,
        Some(client.inbox_id())
    );
    for source in [
        BackendSource::Connected { backend },
        BackendSource::Options {
            options: backend_options,
        },
    ] {
        verify(crate::static_helpers::can_message_with_backend(source, identities.clone()).await?);
    }
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn facade_authorization_and_installation_signatures() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let backend = options().backend.expect("backend options");
    assert!(
        crate::static_helpers::is_address_authorized_with_backend(
            backend.clone(),
            a.inbox_id(),
            a.identity().identifier,
        )
        .await?
    );
    assert!(
        !crate::static_helpers::is_address_authorized_with_backend(
            backend.clone(),
            a.inbox_id(),
            b.identity().identifier,
        )
        .await?
    );
    assert!(
        crate::static_helpers::is_installation_authorized_with_backend(
            backend.clone(),
            a.inbox_id(),
            a.installation_id(),
        )
        .await?
    );
    assert!(
        !crate::static_helpers::is_installation_authorized_with_backend(
            backend,
            a.inbox_id(),
            b.installation_id(),
        )
        .await?
    );

    let text = "installation signature".to_owned();
    let signature = a.sign_with_installation_key(text.clone()).await?;
    assert!(
        a.verify_signed_with_installation_key(text.clone(), signature.clone())
            .await?
    );
    assert!(
        crate::client_identity::verify_signed_with_public_key(
            text,
            signature.clone(),
            a.installation_id_bytes(),
        )
        .await?
    );
    assert!(
        !a.verify_signed_with_installation_key("different text".into(), signature)
            .await?
    );
    a.end().await?;
    b.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn latest_inbox_update_counts_preserve_registered_and_unknown_keys() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let own = client.inbox_id();
    let unknown = InboxId::try_from("00".repeat(32))?;
    assert_ne!(own, unknown);

    let counts = client
        .latest_inbox_updates_count(vec![own.clone(), unknown.clone()], false)
        .await?;
    assert_eq!(
        client.own_inbox_updates_count(true).await?,
        counts[own.checked()?]
    );
    let backend = options().backend.expect("backend options");
    let without_client =
        crate::static_helpers::latest_inbox_updates_count(vec![own.clone()], backend).await?;
    assert_eq!(without_client[own.checked()?], counts[own.checked()?]);
    assert_eq!(counts.len(), 2);
    assert!(counts[own.checked()?] > 0);
    assert_eq!(counts[unknown.checked()?], 0);
    client.end().await?;
}

// The instance `inbox_state` and `inbox_states` use the client's own identity
// route, not the static helper. A local read and a remote read of one inbox
// are equal, and a selected read returns only the selected inboxes.
#[xmtp_common::test(unwrap_try = true)]
async fn instance_inbox_states_read_local_and_selected_inboxes() {
    let signer = crate::generate_local_signer().await;
    let identity = signer::identity(signer.clone()).await?;
    let client = Client::create(signer, options()).await?;
    let local = client.inbox_state(false).await?;
    assert_eq!(local.inbox_id, client.inbox_id());
    let installations: Vec<_> = local
        .installations
        .iter()
        .map(|installation| installation.id.clone())
        .collect();
    assert_eq!(installations, vec![client.installation_id()]);
    assert_eq!(
        format!("{:?}", local.identities),
        format!("{:?}", vec![identity.clone()])
    );
    assert_eq!(
        format!("{:?}", local.recovery_identity),
        format!("{identity:?}")
    );
    assert!(local.creation_signature_kind.is_some());

    let second = Client::create(crate::generate_local_signer().await, options()).await?;
    let mut unregistered = options();
    unregistered.registration.auto = false;
    let reader = Client::create(crate::generate_local_signer().await, unregistered).await?;
    let states = reader
        .inbox_states(vec![client.inbox_id(), second.inbox_id()], true)
        .await?;
    assert_eq!(states.len(), 2);
    let state_of = |id: &InboxId| {
        states
            .iter()
            .find(|state| &state.inbox_id == id)
            .expect("selected inbox state")
    };
    assert_eq!(
        format!("{:?}", state_of(&client.inbox_id())),
        format!("{local:?}")
    );
    assert_eq!(
        format!("{:?}", state_of(&second.inbox_id()).identities),
        format!("{:?}", vec![second.identity()])
    );
    reader.end().await?;
    second.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn facade_key_package_statuses_keep_missing_entries() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let missing = crate::InstallationId::try_from("00".repeat(32))?;
    let own = client.installation_id();
    let entries = client
        .key_package_statuses(vec![own.clone(), missing.clone()])
        .await?;
    assert_eq!(entries.len(), 2);
    assert!(entries[own.checked()?].lifetime.is_some());
    assert!(entries[own.checked()?].validation_error.is_none());
    assert!(entries[missing.checked()?].lifetime.is_none());
    assert!(entries[missing.checked()?].validation_error.is_some());

    let backend_entries = crate::static_helpers::key_package_statuses_with_backend(
        options().backend.expect("backend options"),
        vec![own.clone(), missing.clone()],
    )
    .await?;
    assert_eq!(backend_entries.len(), 2);
    let registered = &backend_entries[own.checked()?];
    let lifetime = registered.lifetime.as_ref().expect("registered package");
    // openmls `Lifetime::default()`: 12 weeks, plus a 1 hour margin before now.
    assert_eq!(
        lifetime.not_after - lifetime.not_before,
        3600 * 24 * 28 * 3 + 3600
    );
    assert!(registered.validation_error.is_none());
    let absent = &backend_entries[missing.checked()?];
    assert!(absent.lifetime.is_none());
    assert_eq!(
        absent.validation_error.as_deref(),
        Some("key package not found")
    );
    client.end().await?;
}

// verifies: API-292, API-293, API-294, API-295
#[xmtp_common::test(unwrap_try = true)]
async fn facade_api_statistics_project_all_seven_shared_counts() {
    use xmtp_proto::api::HasStats;
    let BackendSource::Options {
        options: backend_options,
    } = options().backend?
    else {
        panic!("backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options).await?);
    let mut settings = options();
    settings.registration.auto = false;
    settings.backend = Some(BackendSource::Connected { backend });
    let first = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let second = Client::create(crate::generate_local_signer().await, settings).await?;
    let mut separate = options();
    separate.registration.auto = false;
    let isolated = Client::create(crate::generate_local_signer().await, separate).await?;
    first.diagnostics().clear_statistics().await?;
    isolated.diagnostics().clear_statistics().await?;
    isolated
        .inner
        .context
        .api()
        .api_client
        .mls_stats()
        .publish
        .count_request();
    let stats = first.inner.context.api().api_client.mls_stats();
    let identity = first.inner.context.api().api_client.identity_stats();
    let counters = [
        &stats.publish,
        &stats.query,
        &stats.query_newest,
        &stats.subscribe,
        &stats.subscribe_static,
        &identity.get_inbox_ids,
        &identity.verify_smart_contract_wallet_signatures,
    ];
    for (index, counter) in counters.into_iter().enumerate() {
        for _ in 0..index + 1 {
            counter.count_request();
        }
    }
    async fn vector(client: &Client) -> Result<[u64; 7], XmtpError> {
        let api = client.diagnostics().api_statistics().await?;
        let identity = client.diagnostics().identity_statistics().await?;
        Ok([
            api.publish,
            api.query,
            api.query_newest,
            api.subscribe,
            api.subscribe_static,
            identity.get_inbox_ids,
            identity.verify_smart_contract_wallet_signatures,
        ])
    }
    assert_eq!(vector(&first).await?, [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(vector(&second).await?, [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(vector(&isolated).await?, [1, 0, 0, 0, 0, 0, 0]);
    let summary = first.diagnostics().aggregate_statistics().await?;
    assert_eq!(summary, second.diagnostics().aggregate_statistics().await?);
    // Check each endpoint and value without parsing the summary format.
    for (name, count) in [
        ("publish", 1),
        ("query", 2),
        ("query_newest", 3),
        ("subscribe", 4),
        ("subscribe_static", 5),
        ("get_inbox_ids", 6),
        ("verify_smart_contract_wallet_signatures", 7),
    ] {
        assert!(summary.contains(name), "missing endpoint {name}: {summary}");
        assert!(
            summary.contains(&count.to_string()),
            "missing count {count}: {summary}"
        );
    }
    second.diagnostics().clear_statistics().await?;
    assert_eq!(vector(&first).await?, [0; 7]);
    assert_eq!(vector(&second).await?, [0; 7]);
    assert_eq!(vector(&isolated).await?, [1, 0, 0, 0, 0, 0, 0]);
    for client in [first, second, isolated] {
        client.end().await?;
    }
}

// verifies: API-292, API-293, API-294, API-295
#[xmtp_common::test(unwrap_try = true)]
async fn facade_api_statistics_track_and_clear_requests() {
    let BackendSource::Options {
        options: backend_options,
    } = options().backend?
    else {
        panic!("backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options).await?);
    let mut settings = options();
    let source = BackendSource::Connected { backend };
    settings.backend = Some(source.clone());
    let first = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let second = Client::create(crate::generate_local_signer().await, settings).await?;
    let isolated = Client::create(crate::generate_local_signer().await, options()).await?;
    first.diagnostics().clear_statistics().await?;
    isolated.diagnostics().clear_statistics().await?;
    first.conversations().create_group(vec![], None).await?;
    let availability = second.can_message(vec![first.identity()]).await?;
    assert!(availability[&format!("ethereum:{}", first.identity().identifier)]);
    let published = first.diagnostics().api_statistics().await?.publish;
    let queried = second
        .diagnostics()
        .identity_statistics()
        .await?
        .get_inbox_ids;
    assert!(published > 0);
    assert!(queried > 0);
    assert_eq!(
        second.diagnostics().api_statistics().await?.publish,
        published
    );
    assert_eq!(
        first
            .diagnostics()
            .identity_statistics()
            .await?
            .get_inbox_ids,
        queried
    );
    let static_availability =
        crate::static_helpers::can_message_with_backend(source, vec![first.identity()]).await?;
    assert!(static_availability[&format!("ethereum:{}", first.identity().identifier)]);
    assert_eq!(
        first
            .diagnostics()
            .identity_statistics()
            .await?
            .get_inbox_ids,
        queried + 1
    );
    assert_eq!(
        second
            .diagnostics()
            .identity_statistics()
            .await?
            .get_inbox_ids,
        queried + 1
    );
    isolated.can_message(vec![isolated.identity()]).await?;
    let separate_count = isolated
        .diagnostics()
        .identity_statistics()
        .await?
        .get_inbox_ids;
    assert!(separate_count > 0);
    let summary = first.diagnostics().aggregate_statistics().await?;
    assert!(summary.contains("publish"));
    assert!(summary.contains("get_inbox_ids"));
    second.diagnostics().clear_statistics().await?;
    assert_eq!(first.diagnostics().api_statistics().await?.publish, 0);
    assert_eq!(
        first
            .diagnostics()
            .identity_statistics()
            .await?
            .get_inbox_ids,
        0
    );
    assert_eq!(
        isolated
            .diagnostics()
            .identity_statistics()
            .await?
            .get_inbox_ids,
        separate_count
    );
    for client in [first, second, isolated] {
        client.end().await?;
    }
}

/// `count_messages` passes its list options to the core count, and
/// `last_read_times` keys each read receipt by its sender's inbox id with the
/// receipt's sent time. A text from the same sender is not a read receipt.
#[xmtp_common::test(unwrap_try = true)]
async fn facade_message_counts_and_read_receipt_times() {
    use crate::{ContentTypeId, ListMessagesOptions};
    use xmtp_content_types::ContentCodec;

    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let a_dm = a.conversations().create_dm(b.inbox_id(), None).await?;
    assert_eq!(a_dm.count_messages(None).await?, 0);
    a_dm.send_text("counted".into(), None).await?;
    assert_eq!(a_dm.count_messages(None).await?, 1);
    assert!(a_dm.last_read_times().await?.is_empty());

    b.conversations().sync_all(None).await?;
    let b_dm = b
        .conversations()
        .get_dm_by_inbox_id(a.inbox_id())
        .await?
        .expect("peer DM");
    b_dm.send_text("not a receipt".into(), None).await?;
    a.conversations().sync_all(None).await?;
    assert!(a_dm.last_read_times().await?.is_empty());

    let receipt = xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
        xmtp_content_types::read_receipt::ReadReceipt {},
    )?;
    let receipt_id = b_dm.send(receipt.try_into()?, None).await?;
    a.conversations().sync_all(None).await?;
    let receipt = a
        .conversations()
        .get_message_by_id(receipt_id)
        .await?
        .expect("read receipt");
    assert!(matches!(receipt.0.content, MessageContent::ReadReceipt));

    assert_eq!(a_dm.count_messages(None).await?, 3);
    let receipts_only = ListMessagesOptions {
        content_types: Some(vec![ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "readReceipt".into(),
            version_major: 1,
            version_minor: 0,
        }]),
        ..Default::default()
    };
    assert_eq!(a_dm.count_messages(Some(receipts_only)).await?, 1);

    let times = a_dm.last_read_times().await?;
    assert_eq!(times.len(), 1);
    let b_inbox = b.inbox_id().into_checked()?;
    assert_eq!(times[&b_inbox].0, receipt.0.sent_at.0);
    a.end().await?;
    b.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn facade_long_text_message_round_trips() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = a.conversations().create_dm(b.inbox_id(), None).await?;
    let text = "long message line\n".repeat(6_000);
    let id = dm.send_text(text.clone(), None).await?;
    b.conversations().sync_all(None).await?;
    let received = b
        .conversations()
        .get_message_by_id(id)
        .await?
        .expect("long message");
    assert!(matches!(received.0.content, MessageContent::Text(value) if value == text));
    a.end().await?;
    b.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn facade_hmac_keys_include_duplicate_dms() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = a.conversations().create_dm(b.inbox_id(), None).await?;
    let second = b.conversations().create_dm(a.inbox_id(), None).await?;
    assert_ne!(first.id(), second.id());
    a.conversations().sync_all(None).await?;
    let keys = a.conversations().hmac_keys().await?;
    for id in [first.id(), second.id()] {
        let entry = keys
            .get(id.checked()?)
            .expect("duplicate DM must have HMAC keys");
        assert_eq!(entry.len(), 3);
        assert!(entry.iter().all(|key| key.key.len() == 42));
        assert!(entry.iter().all(|key| key.epoch >= 1));
    }
    a.end().await?;
    b.end().await?;
}
