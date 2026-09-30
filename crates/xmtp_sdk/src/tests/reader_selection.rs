use super::*;
use xmtp_db::{
    delivery::QueryDelivery,
    refresh_state::{EntityKind, QueryRefreshState},
};

// verifies: PROC-046, PROC-047, CONS-042, CONS-043
#[xmtp_common::test(unwrap_try = true)]
async fn fixed_reader_selection_preserves_none_and_empty_filters() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let denied = client.conversations().create_group(vec![], None).await?;
    let unknown = client.conversations().create_group(vec![], None).await?;
    let dm = client
        .conversations()
        .create_dm(peer.inbox_id(), None)
        .await?;
    let beginning = client.conversations().beginning_delivery_cursor().await?;
    let denied_id = denied.send_text("denied".into(), None).await?;
    dm.send_text("different kind".into(), None).await?;
    let unknown_id = unknown.send_text("unknown".into(), None).await?;
    denied
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    unknown
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Unknown)?;
    let reader = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            conversation_kind: Some(crate::ConversationKind::Group),
            ..Default::default()
        }))
        .await?;
    assert_eq!(
        reader
            .next()
            .await?
            .expect("unknown is selected by default")
            .0
            .id,
        unknown_id
    );
    reader.end().await?;
    let named = denied
        .message_reader(Some(crate::ConversationMessageReaderOptions {
            from: Some(beginning.clone()),
        }))
        .await?;
    assert_eq!(
        named
            .next()
            .await?
            .expect("named reader has no consent filter")
            .0
            .id,
        denied_id
    );
    named.end().await?;
    let explicit = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            consent_states: Some(vec![crate::ConsentState::Denied]),
            from: Some(beginning),
            ..Default::default()
        }))
        .await?;
    assert_eq!(
        explicit
            .next()
            .await?
            .expect("explicit denied selection")
            .0
            .id,
        denied_id
    );
    explicit.end().await?;
    let empty = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            consent_states: Some(vec![]),
            ..Default::default()
        }))
        .await?;
    let reading = empty.clone();
    let mut task = tokio::spawn(async move { reading.next().await });
    let db = client.inner.context.db();
    let tail = db.current_delivery_cursor()?.delivery_sequence;
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        tokio::select! {
            result = &mut task => panic!("empty filter returned an item: {result:?}"),
            _ = async {
                loop {
                    if db.get_last_cursor(&unknown.inner.group_id, EntityKind::Delivery).unwrap().0 == tail { break; }
                    tokio::task::yield_now().await;
                }
            } => {}
        }
    }).await?;
    empty.end().await?;
    assert!(task.await??.is_none(), "empty filter must deliver nothing");
    client.end().await?;
    peer.end().await?;
}

// verifies: PROC-047
#[xmtp_common::test(unwrap_try = true)]
async fn scope_exclusion_preserves_d_filter_exclusion_advances_d() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let filtered = client.conversations().create_group(vec![], None).await?;
    let selected = client.conversations().create_group(vec![], None).await?;
    let outside = client.conversations().create_group(vec![], None).await?;
    let filtered_id = filtered.send_text("filtered".into(), None).await?;
    let filtered_cursor = client
        .inner
        .context
        .db()
        .current_delivery_cursor()?
        .delivery_sequence;
    let selected_id = selected.send_text("selected".into(), None).await?;
    let outside_id = outside
        .send_text("outside named scope".into(), None)
        .await?;
    filtered
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    let db = client.inner.context.db();
    let named = selected.message_reader(None).await?;
    assert_eq!(
        named.next().await?.expect("named selection").0.id,
        selected_id
    );
    named.end().await?;
    for group in [&filtered, &selected, &outside] {
        assert_eq!(
            db.get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
                .0,
            0,
            "named scope must not consume other groups or its last handoff"
        );
    }
    let all = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            consent_states: Some(vec![crate::ConsentState::Allowed]),
            ..Default::default()
        }))
        .await?;
    assert_eq!(
        all.next().await?.expect("allowed selection").0.id,
        selected_id
    );
    all.end().await?;
    assert_eq!(
        db.get_last_cursor(&filtered.inner.group_id, EntityKind::Delivery)?
            .0,
        filtered_cursor
    );
    assert_eq!(
        db.get_last_cursor(&outside.inner.group_id, EntityKind::Delivery)?
            .0,
        0
    );
    let new_id = filtered.send_text("after exclusion".into(), None).await?;
    let reopened = filtered.message_reader(None).await?;
    assert_eq!(
        reopened
            .next()
            .await?
            .expect("consumed filter exclusion")
            .0
            .id,
        new_id
    );
    reopened.end().await?;
    let outside_reader = outside.message_reader(None).await?;
    assert_eq!(
        outside_reader
            .next()
            .await?
            .expect("preserved outside backlog")
            .0
            .id,
        outside_id
    );
    outside_reader.end().await?;
    assert!(
        filtered
            .messages(None)
            .await?
            .iter()
            .any(|message| message.0.id == filtered_id)
    );
    client.end().await?;
}
