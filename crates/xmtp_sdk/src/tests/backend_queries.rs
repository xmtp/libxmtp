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

#[xmtp_common::test(unwrap_try = true)]
async fn facade_api_statistics_track_and_clear_requests() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let diagnostics = client.diagnostics();
    diagnostics.clear_statistics().await?;
    client.conversations().create_group(vec![], None).await?;
    let can_message = client.can_message(vec![client.identity()]).await?;
    assert_eq!(can_message.len(), 1);
    assert!(can_message[&format!("ethereum:{}", client.identity().identifier)]);
    let api = diagnostics.api_statistics().await?;
    let identity = diagnostics.identity_statistics().await?;
    assert!(api.publish > 0);
    assert!(identity.get_inbox_ids > 0);
    let aggregate = diagnostics.aggregate_statistics().await?;
    assert!(aggregate.contains("publish"));
    assert!(aggregate.contains("get_inbox_ids"));
    diagnostics.clear_statistics().await?;
    assert_eq!(diagnostics.api_statistics().await?.publish, 0);
    assert_eq!(diagnostics.identity_statistics().await?.get_inbox_ids, 0);
    client.end().await?;
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
