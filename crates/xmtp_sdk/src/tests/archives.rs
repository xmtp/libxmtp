use super::*;

// verifies: STORE-009
#[xmtp_common::test(unwrap_try = true)]
async fn consent_archive_storage_and_diagnostics() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let preferences = client.preferences();
    let entity = ConsentEntity::Inbox {
        inbox_id: client.inbox_id(),
    };
    preferences
        .set_consent_states(vec![ConsentRecord {
            entity: entity.clone(),
            state: ConsentState::Allowed,
        }])
        .await?;
    assert!(matches!(
        preferences.consent_state(entity).await?,
        ConsentState::Allowed
    ));
    assert!(client.storage().path().await?.is_none());
    client.diagnostics().clear_statistics().await?;
    let stats = client.diagnostics().api_statistics().await?;
    assert_eq!(stats.query, 0);
    let archive = client.archives().export_to_bytes(vec![7; 32], None).await?;
    assert!(!archive.is_empty());
    let metadata = client
        .archives()
        .metadata_from_bytes(archive, vec![7; 32])
        .await?;
    assert_eq!(metadata.backup_version, 0);
    client.end().await?;
}

// verifies: ARCH-017
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_empty_archive_elements_export_nothing() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let bytes = client
        .archives()
        .export_to_bytes(
            vec![7; 32],
            Some(crate::ArchiveOptions {
                start: None,
                end: None,
                elements: Some(vec![]),
                exclude_disappearing_messages: false,
            }),
        )
        .await?;
    let metadata = client
        .archives()
        .metadata_from_bytes(bytes, vec![7; 32])
        .await?;
    assert!(metadata.elements.is_empty());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn archive_excludes_disappearing_messages_when_requested() {
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), options()).await?;
    let group = first.conversations().create_group(vec![], None).await?;
    group.send_text("kept".into(), None).await?;
    group
        .update_disappearing_settings(Some(crate::DisappearingSettings {
            from: crate::Timestamp(xmtp_common::time::now_ns()),
            retention_ns: xmtp_common::NS_IN_MIN,
        }))
        .await?;
    group.send_text("excluded".into(), None).await?;
    let archives = first.archives();
    let selection = |exclude_disappearing_messages| crate::ArchiveOptions {
        start: None,
        end: None,
        elements: Some(vec![crate::ArchiveElement::Messages]),
        exclude_disappearing_messages,
    };
    let filtered = archives
        .export_to_bytes(vec![7; 32], Some(selection(true)))
        .await?;
    let second = Client::create(signer, options()).await?;
    second
        .archives()
        .import_from_bytes(filtered, vec![7; 32])
        .await?;
    let crate::Conversation::Group { group: imported } = second
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("archived group")
    else {
        panic!("archive must restore a group");
    };
    let texts = imported
        .messages(None)
        .await?
        .into_iter()
        .filter_map(|message| match message.0.content {
            MessageContent::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, ["kept"]);

    let complete = archives
        .export_to_bytes(vec![8; 32], Some(selection(false)))
        .await?;
    second
        .archives()
        .import_from_bytes(complete, vec![8; 32])
        .await?;
    let texts = imported
        .messages(None)
        .await?
        .into_iter()
        .filter_map(|message| match message.0.content {
            MessageContent::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(texts.len(), 2);
    assert!(texts.contains(&"excluded".to_string()));
    first.end().await?;
    second.end().await?;
}
