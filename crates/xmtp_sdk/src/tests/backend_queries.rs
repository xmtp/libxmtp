use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn backend_only_identity_and_message_queries() {
    let path = std::env::temp_dir().join(format!(
        "sdk-backend-queries-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut settings = options();
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
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
    assert!(availability[&identity.identifier]);
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
    let group_id = group.id().0;
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
async fn facade_key_package_statuses_keep_missing_entries() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let missing = crate::InstallationId::try_from("00".repeat(32))?;
    let own = client.installation_id();
    let entries = client
        .key_package_statuses(vec![own.clone(), missing.clone()])
        .await?;
    assert_eq!(entries.len(), 2);
    assert!(entries[&own.0].lifetime.is_some());
    assert!(entries[&own.0].validation_error.is_none());
    assert!(entries[&missing.0].lifetime.is_none());
    assert!(entries[&missing.0].validation_error.is_some());
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
    assert!(can_message[&client.identity().identifier]);
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

#[xmtp_common::test(unwrap_try = true)]
async fn facade_message_counts_and_last_read_times() {
    use xmtp_content_types::ContentCodec;

    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let a_dm = a.conversations().create_dm(b.inbox_id(), None).await?;
    assert_eq!(a_dm.count_messages(None).await?, 0);
    a_dm.send_text("counted".into(), None).await?;
    assert_eq!(a_dm.count_messages(None).await?, 1);
    b.conversations().sync_all(None).await?;
    let b_dm = b
        .conversations()
        .get_dm_by_inbox_id(a.inbox_id())
        .await?
        .expect("peer DM");
    let receipt = xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
        xmtp_content_types::read_receipt::ReadReceipt {},
    )?;
    b_dm.send(receipt.into(), None).await?;
    a.conversations().sync_all(None).await?;
    let times = a_dm.last_read_times().await?;
    assert_eq!(times.len(), 1);
    assert!(times[&b.inbox_id().0].0 > 0);
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
        let entry = keys.get(&id.0).expect("duplicate DM must have HMAC keys");
        assert_eq!(entry.len(), 3);
        assert!(entry.iter().all(|key| key.key.len() == 42));
        assert!(entry.iter().all(|key| key.epoch >= 1));
    }
    a.end().await?;
    b.end().await?;
}
