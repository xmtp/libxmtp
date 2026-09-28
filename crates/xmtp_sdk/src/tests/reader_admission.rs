use super::*;
use xmtp_db::{
    ConnectionExt,
    diesel::prelude::*,
    refresh_state::{EntityKind, QueryRefreshState},
};

fn gate() -> Arc<reader::HandoffGate> {
    Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    })
}

// verifies: PROC-025
#[xmtp_common::test(unwrap_try = true)]
async fn prepared_message_rechecks_expiry_before_admission() {
    use xmtp_db::schema::group_messages::dsl;
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let stale = group
        .send_text("expired during preparation".into(), None)
        .await?;
    let live = group.send_text("live".into(), None).await?;
    let reader = group.message_reader(None).await?;
    let gate = gate();
    *reader.handoff_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.find(hex::decode(stale.0).unwrap()))
            .set(dsl::expire_at_ns.eq(1_i64))
            .execute(conn)
    })?;
    gate.release.notify_one();
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(10), task)
            .await???
            .expect("live item")
            .0
            .id,
        live
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: PROC-031
#[xmtp_common::test(unwrap_try = true)]
async fn prepared_message_rechecks_owner_before_admission() {
    use xmtp_db::schema::user_preferences::dsl;
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    group.send_text("old owner".into(), None).await?;
    let reader = group.message_reader(None).await?;
    let gate = gate();
    *reader.handoff_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::user_preferences)
            .set(dsl::delivery_owner.eq(Some(vec![99_u8; 16])))
            .execute(conn)
    })?;
    gate.release.notify_one();
    let result = xmtp_common::time::timeout(Duration::from_secs(10), task).await??;
    assert!(
        matches!(result, Err(crate::XmtpError::ConsumerOwned(_))),
        "{result:?}"
    );
    assert_eq!(
        client
            .inner
            .context
            .db()
            .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
            .0,
        0
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: PROC-025, PROC-028, PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_prepared_message_stays_waiting_on_resume() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let stale = client.conversations().create_group(vec![], None).await?;
    let live = client.conversations().create_group(vec![], None).await?;
    stale
        .send_text("cancelled before admission".into(), None)
        .await?;
    let live_id = live.send_text("live".into(), None).await?;
    let reader = client.conversations().message_reader(None).await?;
    let gate = gate();
    *reader.handoff_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    stale
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    gate.release.notify_one();
    let resumed = xmtp_common::time::timeout(Duration::from_secs(10), reader.next())
        .await??
        .expect("current selection");
    assert_eq!(resumed.0.id, live_id);
    reader.end().await?;
    client.end().await?;
}

// verifies: PROC-028, PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn delayed_worker_reply_is_admitted_but_not_acknowledged() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("worker completed".into(), None).await?;
    let reader = client.conversations().message_reader(None).await?;
    let gate = gate();
    *reader.worker_reply_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    assert_eq!(
        client
            .inner
            .context
            .db()
            .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
            .0,
        0
    );
    group
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    gate.release.notify_one();
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(10), task)
            .await???
            .expect("already admitted")
            .0
            .id,
        id
    );
    reader.end().await?;
    assert_eq!(
        client
            .inner
            .context
            .db()
            .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
            .0,
        0
    );
    let replay = group.message_reader(None).await?;
    assert_eq!(
        xmtp_common::time::timeout(Duration::from_secs(10), replay.next())
            .await??
            .expect("unacknowledged")
            .0
            .id,
        id
    );
    replay.end().await?;
    client.end().await?;
}
