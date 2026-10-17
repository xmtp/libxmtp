use super::*;

// verifies: DMS-017, PROC-052, PROC-034, PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn foreign_restored_dm_has_no_local_peer() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let path = std::env::temp_dir().join(format!(
        "sdk-restored-peer-{}.db3",
        xmtp_common::time::now_ns()
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let charlie = Client::create(signer.clone(), settings.clone()).await?;
    let dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    let other = bo.conversations().create_dm(alix.inbox_id(), None).await?;
    assert_ne!(dm.id(), other.id());
    assert_eq!(other.peer_inbox_id().await?, Some(alix.inbox_id()));
    alix.conversations().sync_all(None).await?;
    let id = dm.send_text("archived peer pair".into(), None).await?;
    assert_eq!(dm.peer_inbox_id().await?, Some(bo.inbox_id()));
    let archive = alix
        .archives()
        .export_to_bytes(
            vec![9; 32],
            Some(crate::ArchiveOptions {
                elements: Some(vec![crate::ArchiveElement::Messages]),
                start: None,
                end: None,
                exclude_disappearing_messages: false,
            }),
        )
        .await?;
    charlie
        .archives()
        .import_from_bytes(archive, vec![9; 32])
        .await?;
    let crate::Conversation::Dm { dm: restored } = charlie
        .conversations()
        .get_by_id(dm.id())
        .await?
        .expect("restored DM")
    else {
        panic!("restored conversation must be a DM");
    };
    assert_eq!(restored.peer_inbox_id().await?, None);
    let history = restored.messages(None).await?;
    let message = history
        .iter()
        .find(|message| message.0.id == id)
        .expect("imported message");
    let cursor = message.0.delivery_cursor.clone().expect("imported cursor");
    let listed = charlie
        .conversations()
        .list_dms(Some(crate::ListConversationsOptions {
            include_duplicate_dms: true,
            ..Default::default()
        }))
        .await?;
    assert_eq!(listed.len(), 2);
    for dm in listed {
        assert_eq!(dm.peer_inbox_id().await?, None);
    }
    let duplicates = restored.duplicate_dms().await?;
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].id(), other.id());
    assert_eq!(duplicates[0].peer_inbox_id().await?, None);
    assert!(!restored.inner.is_active()?);
    assert!(matches!(
        restored.inner.sync().await,
        Err(xmtp_mls::groups::GroupError::GroupInactive)
    ));
    assert!(restored.send_text("inactive".into(), None).await.is_err());
    assert!(restored.sync().await.is_err());
    let beginning = charlie.conversations().beginning_delivery_cursor().await?;
    let replay = restored
        .message_reader(Some(crate::ConversationMessageReaderOptions {
            from: Some(beginning),
        }))
        .await?;
    let reader = restored.message_reader(None).await?;
    for item in [
        replay.next().await?.expect("replay"),
        reader.next().await?.expect("default"),
    ] {
        assert_eq!(item.0.id, id);
        assert_eq!(item.0.delivery_cursor.as_ref(), Some(&cursor));
    }
    replay.end().await?;
    reader.end().await?;
    let restored_id = restored.id();
    charlie.end().await?;
    drop((reader, replay, restored, duplicates, charlie));
    let reopened = Client::create(signer, settings).await?;
    let crate::Conversation::Dm { dm } = reopened
        .conversations()
        .get_by_id(restored_id)
        .await?
        .expect("restored DM")
    else {
        panic!("DM")
    };
    assert_eq!(dm.peer_inbox_id().await?, None);
    let reader = dm.message_reader(None).await?;
    let repeated = reader
        .next()
        .await?
        .expect("unacknowledged imported message");
    assert_eq!(repeated.0.id, id);
    assert_eq!(repeated.0.delivery_cursor.as_ref(), Some(&cursor));
    reader.end().await?;
    reopened.end().await?;
    drop((reader, dm, reopened));
    std::fs::remove_file(path)?;
    bo.end().await?;
    alix.end().await?;
}
