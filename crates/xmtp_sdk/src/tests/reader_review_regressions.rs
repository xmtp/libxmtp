use super::*;
use xmtp_db::{
    ConnectionExt,
    refresh_state::{EntityKind, QueryRefreshState},
};

// verifies: PROC-052, PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn reader_review_cancel_after_load_rechecks_scope() {
    cancel_after_load(false).await?;
}

// verifies: PROC-052, PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn reader_review_cancel_after_load_rechecks_consent() {
    cancel_after_load(true).await?;
}

async fn cancel_after_load(consent: bool) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let stale = client.conversations().create_group(vec![], None).await?;
    let live = client.conversations().create_group(vec![], None).await?;
    let first_id = stale.send_text("A already handled".into(), None).await?;
    let reader = client.conversations().message_reader(None).await?;
    let first = reader.next().await?.expect("A handoff");
    assert_eq!(first.0.id, first_id);
    let first_cursor =
        crate::delivery::cursor::parse(first.0.delivery_cursor.as_deref().expect("A cursor"))?;
    let excluded_id = stale
        .send_text("B available before cancellation".into(), None)
        .await?;
    let live_id = live.send_text("C current selection".into(), None).await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    let settled = Arc::new(Notify::new());
    {
        let mut gates = reader.request_gates.lock();
        gates.after_cancel_check = Some(gate.clone());
        gates.settled = Some(settled.clone());
    }
    let reading = reader.clone();
    let task = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    gate.release.notify_one();
    xmtp_common::time::timeout(Duration::from_secs(10), settled.notified()).await?;
    *reader.request_gates.lock() = reader::RequestGates::default();
    let committed = client
        .inner
        .context
        .db()
        .get_last_cursor(&stale.inner.group_id, EntityKind::Delivery)?
        .0;
    if consent {
        stale
            .inner
            .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    } else {
        reader.update_scope_for_test(vec![live.inner.group_id]);
    }
    let resumed = xmtp_common::time::timeout(Duration::from_secs(10), reader.next())
        .await??
        .expect("current selection");
    reader.end().await?;
    client.end().await?;
    assert_eq!(
        committed, first_cursor.delivery_sequence,
        "A's committed ACK must remain"
    );
    assert_ne!(
        resumed.0.id, excluded_id,
        "cancelled B must stay unadmitted"
    );
    assert_eq!(resumed.0.id, live_id, "resume must apply the new selection");
    Ok(())
}

// verifies: PROC-052, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn reader_review_callback_commit_failure_replays_on_same_connection() {
    use xmtp_db::diesel::{
        Connection, RunQueryDsl,
        connection::{SimpleConnection, TransactionManager},
        sql_query,
    };
    use xmtp_mls::subscriptions::{
        local_delivery::{DeliveryScope, LocalDeliveryError, LocalDeliveryFilter},
        message_reader::MessageReader as CoreReader,
    };
    let root = temp_root("callback-ack-commit-failure");
    std::fs::create_dir_all(&root)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("client.db3"));
    settings.storage.single_connection = true;
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let first_id = group.send_text("callback A".into(), None).await?;
    group
        .send_text("B must not skip failed A".into(), None)
        .await?;
    let mut core = CoreReader::new(
        client.inner.context.clone(),
        DeliveryScope::Groups(vec![group.inner.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    let item = core.next_delivery().await?.expect("callback A");
    item.acknowledgement.check_owner()?;
    client.inner.context.db().raw_query(|conn| conn.batch_execute(
        "CREATE TABLE callback_commit_parent (id INTEGER PRIMARY KEY); \
         CREATE TABLE callback_commit_child (id INTEGER PRIMARY KEY, parent INTEGER REFERENCES callback_commit_parent(id) DEFERRABLE INITIALLY DEFERRED); \
         CREATE TRIGGER fail_callback_ack AFTER INSERT ON refresh_state WHEN NEW.entity_kind = 10 BEGIN INSERT INTO callback_commit_child VALUES (1, 1); END"
    ))?;
    let result = item.acknowledgement.acknowledge();
    let transaction_open = client.inner.context.db().raw_query(|conn| {
        Ok:: <_, xmtp_db::diesel::result::Error>(<xmtp_db::diesel::SqliteConnection as Connection>::TransactionManager::transaction_manager_status_mut(conn).transaction_depth()?.is_some())
    })?;
    let d = client
        .inner
        .context
        .db()
        .get_refresh_state(&group.inner.group_id, EntityKind::Delivery)?
        .map_or(0, |row| row.sequence_id);
    client
        .inner
        .context
        .db()
        .raw_query(|conn| sql_query("DROP TRIGGER fail_callback_ack").execute(conn))?;
    core.close();
    drop((item, core));
    let replacement = group.message_reader(None).await?;
    let replay = xmtp_common::time::timeout(Duration::from_secs(10), replacement.next())
        .await??
        .expect("same-client replay");
    replacement.end().await?;
    client.end().await?;
    println!(
        "CALLBACK_COMMIT_FAILURE transaction_open={transaction_open} d={d} replay_a={}",
        replay.0.id == first_id
    );
    let storage_cause = match &result {
        Err(LocalDeliveryError::Storage(_)) => true,
        Err(LocalDeliveryError::SessionFailure(cause)) => {
            matches!(cause.as_ref(), LocalDeliveryError::Storage(_))
        }
        _ => false,
    };
    assert!(storage_cause, "typed callback storage cause: {result:?}");
    assert!(
        !transaction_open,
        "failed callback COMMIT must close its transaction"
    );
    assert_eq!(d, 0, "failed callback ACK must leave D unchanged");
    assert_eq!(replay.0.id, first_id, "same connection must replay A");
    std::fs::remove_dir_all(root)?;
}
