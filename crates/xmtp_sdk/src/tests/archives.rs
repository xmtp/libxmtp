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

// verifies: ARCH-015, ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn list_and_get_keep_restored_groups_readable() {
    use xmtp_mls::groups::MlsGroup;
    use xmtp_mls::mls_common::group_metadata::DmMembers;
    use xmtp_proto::xmtp::device_sync::group_backup::{GroupSave, ImmutableMetadataSave};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let own = client.inbox_id().into_checked()?;
    let live = client.conversations().create_group(vec![], None).await?;
    // Legacy version-0 shapes: no metadata message and no adder; a present
    // empty creator with a known adder; a DM with no metadata and no adder.
    let unknown = GroupSave {
        id: vec![0x91; 16],
        conversation_type: 1,
        ..Default::default()
    };
    let empty = GroupSave {
        id: vec![0x92; 16],
        conversation_type: 1,
        added_by_inbox_id: "archived-adder".into(),
        metadata: Some(ImmutableMetadataSave {
            creator_inbox_id: String::new(),
        }),
        ..Default::default()
    };
    let dm = GroupSave {
        id: vec![0x93; 16],
        conversation_type: 2,
        dm_id: Some(
            DmMembers {
                member_one_inbox_id: own.clone(),
                member_two_inbox_id: "b".repeat(64),
            }
            .to_string(),
        ),
        ..Default::default()
    };
    for save in [&unknown, &empty, &dm] {
        MlsGroup::<xmtp_mls::MlsContext>::restore_from_archive(&client.inner.context, save)?;
    }

    let listed = client.conversations().list(None).await?;
    assert_eq!(listed.len(), 4);
    for (save, adder) in [
        (&unknown, None),
        (&empty, Some("archived-adder")),
        (&dm, None),
    ] {
        let conversation_id = ConversationId::try_from(hex::encode(&save.id))?;
        let fetched = client
            .conversations()
            .get_by_id(conversation_id.clone())
            .await?
            .expect("restored conversation");
        let listed = listed_conversation(&listed, &conversation_id);
        for conversation in [&fetched, listed] {
            let (creator, added_by, is_creator) = received_identity(conversation);
            // Exact archived creator projection is deferred. Both paths use
            // the same placeholder and still preserve the archived adder.
            assert_eq!(creator.as_deref(), Some(own.as_str()));
            assert!(is_creator);
            assert_eq!(added_by.as_deref(), adder);
        }
        assert_eq!(conversation_messages_count(&fetched).await?, 0);
        let result = match &fetched {
            crate::Conversation::Group { group } => group.send_text("inactive".into(), None).await,
            crate::Conversation::Dm { dm } => dm.send_text("inactive".into(), None).await,
        };
        assert!(
            result.is_err(),
            "a placeholder creator must not authorize a send"
        );
    }
    let (creator, _, is_creator) = received_identity(listed_conversation(&listed, &live.id()));
    assert_eq!(creator.as_deref(), Some(own.as_str()));
    assert!(is_creator);
    client.end().await?;
}

fn listed_conversation<'a>(
    listed: &'a [crate::Conversation],
    id: &ConversationId,
) -> &'a crate::Conversation {
    listed
        .iter()
        .find(|conversation| match conversation {
            crate::Conversation::Group { group } => group.id() == *id,
            crate::Conversation::Dm { dm } => dm.id() == *id,
        })
        .expect("conversation in list")
}

/// The creator, the adder, and `is_creator`. An unknown ID is empty text,
/// which the checked accessor rejects, so it reads as `None`.
fn received_identity(conversation: &crate::Conversation) -> (Option<String>, Option<String>, bool) {
    let read = |id: Option<crate::InboxId>| id.and_then(|id| id.into_checked().ok());
    match conversation {
        crate::Conversation::Group { group } => (
            read(group.creator_inbox_id()),
            read(group.added_by_inbox_id()),
            group.is_creator(),
        ),
        crate::Conversation::Dm { dm } => (
            read(dm.creator_inbox_id()),
            read(dm.added_by_inbox_id()),
            dm.is_creator(),
        ),
    }
}

async fn conversation_messages_count(
    conversation: &crate::Conversation,
) -> Result<usize, XmtpError> {
    Ok(match conversation {
        crate::Conversation::Group { group } => group.messages(None).await?.len(),
        crate::Conversation::Dm { dm } => dm.messages(None).await?.len(),
    })
}
