use super::*;
use crate::client::build_task_probe::{BuildTaskProbe, CURRENT};
use xmtp_db::ConnectionExt;
use xmtp_db::diesel::{RunQueryDsl, sql_query};

async fn completed_constructor_is_cancelled(build: bool) -> Result<(), xmtp_common::BoxDynError> {
    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-adoption-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    std::fs::create_dir_all(&directory)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&directory.join("client.db3"));
    settings.storage.single_connection = true;
    let signer = crate::generate_local_signer().await;
    let identity = signer.identity().await?;
    if build {
        Client::create(signer.clone(), settings.clone())
            .await?
            .end()
            .await?;
    }
    let probe = Arc::new(BuildTaskProbe::default());
    let constructor = async move {
        if build {
            Client::build(identity, settings, None).await
        } else {
            Client::create(signer, settings).await
        }
    };
    let mut constructor = Box::pin(CURRENT.scope(probe.clone(), constructor));
    tokio::select! {
        _ = &mut constructor => panic!("constructor returned before the task probe"),
        _ = probe.started.notified() => {}
    }
    // Do not poll the constructor again. Only the spawned task may run.
    xmtp_common::time::timeout(Duration::from_secs(20), async {
        loop {
            if probe
                .task
                .lock()
                .as_ref()
                .expect("spawned task")
                .is_finished()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let core = probe
        .client
        .lock()
        .take()
        .expect("completed successful client");
    assert!(!crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed));
    assert!(!core.context.is_closed());
    assert!(
        core.context
            .db()
            .raw_query(|conn| sql_query("SELECT 1").execute(conn))
            .is_ok()
    );
    drop(constructor);
    let reported_open = crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed);
    let cleaned = xmtp_common::time::timeout(Duration::from_secs(3), async {
        while !core.context.shutdown_complete() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    let disconnected = core
        .context
        .db()
        .raw_query(|conn| sql_query("SELECT 1").execute(conn))
        .is_err();
    // Also close the client under a broken implementation before asserting.
    core.close().await?;
    std::fs::remove_dir_all(directory)?;
    assert!(reported_open, "unadopted client was not reported open");
    assert!(cleaned, "unadopted client workers did not stop");
    assert!(disconnected, "unadopted client store stayed connected");
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn completed_create_cancelled_before_adoption_closes_client() {
    completed_constructor_is_cancelled(false).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn completed_build_cancelled_before_adoption_closes_client() {
    completed_constructor_is_cancelled(true).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn adopted_create_and_build_keep_the_client_open() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-adopted-{}.db3",
        xmtp_common::time::now_ns()
    ));
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    settings.storage.single_connection = true;
    let signer = crate::generate_local_signer().await;
    let identity = signer.identity().await?;
    let created = Client::create(signer, settings.clone()).await?;
    assert!(!crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed));
    assert!(!created.inner.context.is_closed());
    assert!(
        created
            .inner
            .context
            .db()
            .raw_query(|conn| sql_query("SELECT 1").execute(conn))
            .is_ok()
    );
    created.end().await?;
    let built = Client::build(identity, settings, None).await?;
    assert!(!crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed));
    assert!(!built.inner.context.is_closed());
    assert!(
        built
            .inner
            .context
            .db()
            .raw_query(|conn| sql_query("SELECT 1").execute(conn))
            .is_ok()
    );
    built.end().await?;
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn failed_create_and_build_do_not_report_closed_stores_open() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-failed-adoption-{}.db3",
        xmtp_common::time::now_ns()
    ));
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    settings.storage.single_connection = true;
    let signer: Arc<dyn Signer> = Arc::new(KindFailsSigner(PrivateKeySigner::random()));
    let identity = signer.identity().await?;
    let probe = Arc::new(BuildTaskProbe::default());
    assert!(
        CURRENT
            .scope(probe.clone(), Client::create(signer, settings.clone()))
            .await
            .is_err()
    );
    let core = probe
        .client
        .lock()
        .take()
        .expect("failed registration client");
    assert!(core.context.shutdown_complete());
    assert!(
        core.context
            .db()
            .raw_query(|conn| sql_query("SELECT 1").execute(conn))
            .is_err()
    );
    assert!(!crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed));
    settings.storage.location = explicit_location(&path.with_extension("missing.db3"));
    assert!(Client::build(identity, settings, None).await.is_err());
    assert!(!crate::client::STORE_LEFT_OPEN.load(Ordering::Relaxed));
    std::fs::remove_file(path)?;
}
