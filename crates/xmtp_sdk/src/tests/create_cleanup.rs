use super::*;

// A failed create discards its client. When close fails, the store must still
// disconnect, because the browser host releases the storage lock next.
#[xmtp_common::test(unwrap_try = true)]
async fn discard_disconnects_store_when_close_fails() {
    use xmtp_db::ConnectionExt;
    use xmtp_db::diesel::{RunQueryDsl, sql_query};

    let mut settings = options();
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-discard-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    // The reader holds the delivery lease that close must release.
    let _reader = group.message_reader(None).await?;
    client.inner.context.db().raw_query(|conn| {
        sql_query(
            "CREATE TRIGGER fail_delivery_release BEFORE UPDATE OF delivery_owner \
             ON user_preferences WHEN NEW.delivery_owner IS NULL \
             BEGIN SELECT RAISE(ABORT, 'injected release failure'); END",
        )
        .execute(conn)
    })?;
    assert!(client.end().await.is_err(), "close did not fail");

    client.discard().await?;

    let query = client
        .inner
        .context
        .db()
        .raw_query(|conn| sql_query("SELECT 1").execute(conn));
    assert!(query.is_err(), "discard left the store connected");
    let _ = std::fs::remove_file(&path);
}

// When close and disconnect both fail, the store of a failed create stays
// open. Discard must report it, so the browser worker keeps the storage lock.
#[xmtp_common::test(unwrap_try = true)]
async fn discard_reports_store_left_open_when_disconnect_fails() {
    use std::sync::atomic::Ordering;
    use xmtp_db::ConnectionExt;
    use xmtp_db::diesel::{RunQueryDsl, sql_query};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    // The reader holds the delivery lease that close must release.
    let _reader = group.message_reader(None).await?;
    client.inner.context.db().raw_query(|conn| {
        sql_query(
            "CREATE TRIGGER fail_delivery_release BEFORE UPDATE OF delivery_owner \
             ON user_preferences WHEN NEW.delivery_owner IS NULL \
             BEGIN SELECT RAISE(ABORT, 'injected release failure'); END",
        )
        .execute(conn)
    })?;
    crate::client::FAIL_DISCARD_DISCONNECT.store(true, Ordering::Relaxed);

    assert!(client.discard().await.is_err(), "discard hid an open store");
    assert!(
        crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed),
        "the open store was not reported"
    );
    client
        .inner
        .context
        .db()
        .raw_query(|conn| sql_query("DROP TRIGGER fail_delivery_release").execute(conn))?;
    client.end().await?;
}

// A cancelled create drops its future without the cleanup of a failed create.
// Its store is already open while the signer is pending, so the drop must
// report the store open. The browser worker then keeps its storage lock.
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_create_reports_store_left_open() {
    use crate::client::build_task_probe::{BuildTaskProbe, CURRENT};
    use std::sync::atomic::Ordering;

    let kind_started = Arc::new(Notify::new());
    let kind_release = Arc::new(Notify::new());
    let signer: Arc<dyn Signer> = Arc::new(PendingKindSigner {
        inner: crate::generate_local_signer().await,
        kind_started: kind_started.clone(),
        kind_release: kind_release.clone(),
    });
    let mut settings = options();
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-cancelled-create-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    settings.storage.location = explicit_location(&path);
    let probe = Arc::new(BuildTaskProbe::default());
    let mut create = Box::pin(CURRENT.scope(probe.clone(), Client::create(signer, settings)));
    tokio::select! {
        _ = &mut create => panic!("create finished while its signer was pending"),
        _ = kind_started.notified() => {}
    }
    assert!(
        !crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed),
        "the store was reported open before the create was cancelled"
    );

    let core = probe
        .client
        .lock()
        .take()
        .expect("client awaiting registration");
    drop(create);
    // The foreign call runs on a blocking thread that the runtime waits for.
    kind_release.notify_one();

    assert!(
        crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed),
        "a cancelled create did not report its open store"
    );
    xmtp_common::time::timeout(Duration::from_secs(5), async {
        while !core.context.shutdown_complete() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled registration left its client running");
    let _ = std::fs::remove_file(&path);
}
