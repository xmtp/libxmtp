//! Configuration admission while the metadata catalogue hook is set.

use super::*;
use crate::{BackendOptions, BackendSource, ClientOptions, StorageLocation};
use xmtp_db::prelude::QueryServerConfiguration;

async fn stored_client() -> (Client, ClientOptions, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "sdk-catalogue-admission-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut settings = options();
    settings.storage.location = StorageLocation::Explicit {
        db_path: path.to_string_lossy().into_owned(),
        attachments_dir: path
            .with_extension("attachments")
            .to_string_lossy()
            .into_owned(),
    };
    let client = Client::create(crate::generate_local_signer().await, settings.clone())
        .await
        .expect("stored client");
    (client, settings, path)
}

// verifies: CONF-064, CONF-072
#[xmtp_common::test(unwrap_try = true)]
async fn catalogue_override_rejects_recorded_backend_conflict() {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    let (online, settings, path) = stored_client().await;
    let identity = online.identity();
    let inbox_id = online.inbox_id();
    online
        .inner
        .context
        .db()
        .record_server_configuration_conflict("other-deployment")?;
    online.end().await?;
    use_application_components(Some(alix_catalogue()))?;
    let result = Client::build(identity, settings, Some(inbox_id)).await;
    if let Ok(client) = &result {
        client.end().await?;
    }
    std::fs::remove_file(&path)?;
    std::fs::remove_dir_all(path.with_extension("attachments"))?;
    assert!(
        matches!(result, Err(XmtpError::BackendMismatch(_))),
        "{:?}",
        result.as_ref().err()
    );
}

// verifies: CONF-030, CONF-033, CONF-064
#[xmtp_common::test(unwrap_try = true)]
async fn catalogue_override_rejects_another_backend() {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    let (online, settings, path) = stored_client().await;
    let identity = online.identity();
    let inbox_id = online.inbox_id();
    let db = online.inner.context.db();
    let stored = db.server_configuration()?.expect("stored configuration");
    db.store_server_configuration(
        "other-deployment",
        "http://moved.example",
        &stored.response,
        stored.fetched_at_ns,
    )?;
    drop(db);
    online.end().await?;
    use_application_components(Some(alix_catalogue()))?;
    let result = Client::build(identity, settings, Some(inbox_id)).await;
    if let Ok(client) = &result {
        client.end().await?;
    }
    std::fs::remove_file(&path)?;
    std::fs::remove_dir_all(path.with_extension("attachments"))?;
    assert!(
        matches!(result, Err(XmtpError::BackendMismatch(_))),
        "{:?}",
        result.as_ref().err()
    );
}

// verifies: CONF-034, CONF-076
#[xmtp_common::test(unwrap_try = true)]
async fn catalogue_override_keeps_offline_configuration() {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    let (online, mut settings, path) = stored_client().await;
    let identity = online.identity();
    let inbox_id = online.inbox_id();
    let stored = online
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("stored configuration");
    online.end().await?;
    settings.allow_offline = true;
    settings.backend = Some(BackendSource::Options {
        options: BackendOptions {
            url: "http://127.0.0.1:1".into(),
            ..Default::default()
        },
    });
    use_application_components(Some(alix_catalogue()))?;
    let offline = Client::build(identity, settings, Some(inbox_id)).await?;
    assert_eq!(offline.server_configuration().identifier, stored.identifier);
    assert_eq!(
        offline.server_configuration().application_components.len(),
        alix_catalogue().len()
    );
    let retained = offline
        .inner
        .context
        .db()
        .server_configuration()?
        .expect("retained configuration");
    assert_eq!(retained.backend_url, stored.backend_url);
    assert_eq!(retained.fetched_at_ns, stored.fetched_at_ns);
    offline.end().await?;
    std::fs::remove_file(&path)?;
    std::fs::remove_dir_all(path.with_extension("attachments"))?;
}

// verifies: CONF-064, CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn catalogue_override_keeps_offline_backend_preflight() {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    let (online, mut settings, path) = stored_client().await;
    let identity = online.identity();
    let inbox_id = online.inbox_id();
    let db = online.inner.context.db();
    let stored = db.server_configuration()?.expect("stored configuration");
    db.store_server_configuration(
        "other-deployment",
        "http://moved.example",
        &stored.response,
        stored.fetched_at_ns,
    )?;
    drop(db);
    online.end().await?;
    settings.allow_offline = true;
    use_application_components(Some(alix_catalogue()))?;
    let offline = Client::build(identity, settings, Some(inbox_id)).await?;
    let error = offline.inbox_state(true).await.unwrap_err();
    offline.end().await?;
    std::fs::remove_file(&path)?;
    std::fs::remove_dir_all(path.with_extension("attachments"))?;
    assert!(matches!(error, XmtpError::BackendMismatch(_)), "{error:?}");
}
