use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn slice_create_send_read_stream_end() {
    let alix = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let bo = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    bo.inner.sync_welcomes().await?;
    let bo_group = crate::Group::from_core(bo.inner.group(&group.inner.group_id)?, bo.key).await?;
    let id = group.send_text("hello from the slice".into(), None).await?;
    let history = group.messages(None).await?;
    let sent = history
        .into_iter()
        .find(|message| message.0.id == id)
        .expect("sent message");
    assert_eq!(sent.0.sender_inbox_id, alix.inbox_id());
    assert_eq!(sent.0.client_key, alix.key);
    assert!(sent.0.sent_at.0 > 0);
    assert!(
        matches!(sent.0.content, MessageContent::Text(ref text) if text == "hello from the slice")
    );

    let reader = bo_group.message_reader(None).await?;
    let received = xmtp_common::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if let Some(message) = reader.next().await?
                && message.0.id == id
            {
                break Ok::<_, crate::XmtpError>(message);
            }
        }
    })
    .await??;
    assert_eq!(received.0.client_key, bo.key);
    assert_eq!(received.0.sender_inbox_id, alix.inbox_id());
    reader.end().await?;
    reader.end().await?;
    alix.end().await?;
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancel_idle_read_settles() {
    let client = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    let idle = reader.idle_read_for_test();
    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    assert!(!pending.is_finished(), "reader did not reach an idle read");
    xmtp_common::time::timeout(Duration::from_secs(2), reader.end()).await??;
    assert!(
        xmtp_common::time::timeout(Duration::from_secs(2), pending)
            .await???
            .is_none(),
        "the idle read did not settle after end"
    );
    assert!(reader.next().await?.is_none());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn client_end_settles_idle_message_read() {
    use xmtp_common::StreamHandle;

    const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
    const END_TIMEOUT: Duration = Duration::from_secs(2);

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    let idle = reader.idle_read_for_test();
    let waiting = reader.clone();
    let (settled, mut settlement) = tokio::sync::oneshot::channel();
    let pending = xmtp_common::spawn(None, async move {
        let result = waiting.next().await;
        let _ = settled.send(());
        result
    });
    xmtp_common::time::timeout(IDLE_TIMEOUT, idle.notified()).await?;
    assert!(
        matches!(
            settlement.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ),
        "the native read must be active before client end"
    );
    xmtp_common::time::timeout(END_TIMEOUT, client.end()).await??;
    assert!(
        xmtp_common::time::timeout(END_TIMEOUT, pending.join())
            .await???
            .is_none(),
        "client end must settle the idle message read"
    );
    assert!(
        settlement.try_recv().is_ok(),
        "the native worker must settle"
    );
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_message_read_delivers_and_replays_unacknowledged_item() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    let idle = reader.idle_read_for_test();
    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));

    let message_id = group.send_text("after cancellation".into(), None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("message after cancelled read");
    assert_eq!(delivered.0.id, message_id);
    reader.end().await?;

    let replay = group.message_reader(None).await?;
    let repeated = xmtp_common::time::timeout(Duration::from_secs(5), replay.next())
        .await??
        .expect("message was not acknowledged");
    assert_eq!(repeated.0.id, message_id);
    replay.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_conversation_read_delivers_next_group() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client.conversations().conversation_reader(None).await?;
    let idle = reader.idle_read_for_test();
    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));

    let group = client.conversations().create_group(vec![], None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("group after cancelled read");
    assert!(
        matches!(delivered, crate::Conversation::Group { group: found } if found.id() == group.id())
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: CONS-030, CONS-044
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_reader_default_includes_denied() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client.conversations().conversation_reader(None).await?;
    let denied = client.conversations().create_group(vec![], None).await?;
    client
        .preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: ConsentEntity::Conversation {
                conversation_id: denied.id(),
            },
            state: ConsentState::Denied,
        }])
        .await?;
    let allowed = client.conversations().create_group(vec![], None).await?;
    let unknown = client.conversations().create_group(vec![], None).await?;
    client
        .preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: ConsentEntity::Conversation {
                conversation_id: unknown.id(),
            },
            state: ConsentState::Unknown,
        }])
        .await?;
    let first = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("denied group");
    assert!(matches!(first, crate::Conversation::Group { group } if group.id() == denied.id()));
    let second = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("allowed group");
    assert!(matches!(second, crate::Conversation::Group { group } if group.id() == allowed.id()));
    let third = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("unknown group");
    assert!(matches!(third, crate::Conversation::Group { group } if group.id() == unknown.id()));
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: CONS-030
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_reader_explicit_allowed_selection() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState, ConversationReaderOptions};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .conversations()
        .conversation_reader(Some(ConversationReaderOptions {
            consent_states: Some(vec![ConsentState::Allowed]),
            ..Default::default()
        }))
        .await?;
    let denied = client.conversations().create_group(vec![], None).await?;
    client
        .preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: ConsentEntity::Conversation {
                conversation_id: denied.id(),
            },
            state: ConsentState::Denied,
        }])
        .await?;
    let allowed = client.conversations().create_group(vec![], None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("allowed group");
    assert!(
        matches!(delivered, crate::Conversation::Group { group } if group.id() == allowed.id())
    );
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(100), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: CONS-042
#[xmtp_common::test(unwrap_try = true)]
async fn all_scope_message_reader_skips_synced_denied_message() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let denied = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let allowed = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    bo.inner.sync_welcomes().await?;
    let bo_denied =
        crate::Group::from_core(bo.inner.group(&denied.inner.group_id)?, bo.key).await?;
    let bo_allowed =
        crate::Group::from_core(bo.inner.group(&allowed.inner.group_id)?, bo.key).await?;
    bo.preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: ConsentEntity::Conversation {
                conversation_id: bo_denied.id(),
            },
            state: ConsentState::Denied,
        }])
        .await?;
    let denied_id = denied.send_text("denied".into(), None).await?;
    bo_denied.sync().await?;
    assert_eq!(
        bo_denied.inner.consent_state()?,
        ConsentState::Denied.into()
    );
    let allowed_id = allowed.send_text("allowed".into(), None).await?;
    bo_allowed.sync().await?;

    let reader = bo_allowed.message_reader(None).await?;
    reader.update_all_scope_for_test();
    let selected = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            let message = reader.next().await?.expect("allowed message");
            assert_ne!(message.0.id, denied_id, "denied message was delivered");
            if message.0.id == allowed_id {
                break Ok::<_, crate::XmtpError>(message);
            }
        }
    })
    .await??;
    assert_eq!(selected.0.id, allowed_id);
    reader.end().await?;
    alix.end().await?;
    bo.end().await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn stream_ack_only_on_next_request() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let first_id = group.send_text("first".into(), None).await?;
    let reader = group.message_reader(None).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
            .await??
            .expect("first item")
            .0
            .id,
        first_id
    );
    reader.end().await?;

    let replay = group.message_reader(None).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(5), replay.next())
            .await??
            .expect("unacknowledged item")
            .0
            .id,
        first_id
    );
    let second_id = group.send_text("second".into(), None).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(5), replay.next())
            .await??
            .expect("second item")
            .0
            .id,
        second_id
    );
    replay.end().await?;

    let remaining = group.message_reader(None).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(5), remaining.next())
            .await??
            .expect("last item")
            .0
            .id,
        second_id
    );
    remaining.end().await?;
    client.end().await?;
}
