use super::*;

// verifies: DMS-017
#[xmtp_common::test(unwrap_try = true)]
async fn foreign_restored_dm_has_no_local_peer() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let charlie = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
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
    assert_eq!(restored.messages(None).await?[0].0.id, id);
    charlie.end().await?;
    bo.end().await?;
    alix.end().await?;
}
