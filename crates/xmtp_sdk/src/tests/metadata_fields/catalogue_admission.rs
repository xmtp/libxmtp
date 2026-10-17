//! Configuration admission while the catalogue override is set. The override
//! replaces only the catalogue of the resolved configuration, so a recorded
//! backend conflict must still stop the build before it sends a request.

use super::*;
use crate::tests::storage_layout::CountingRelay;
use xmtp_db::prelude::QueryServerConfiguration;

// verifies: CONF-064, CONF-072
#[xmtp_common::test(unwrap_try = true)]
async fn catalogue_override_rejects_recorded_backend_conflict_without_a_request() {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    let relay = CountingRelay::start().await?;
    let path = temp_root("catalogue-recorded-conflict").with_extension("db3");
    let mut settings = options();
    settings.backend = relay.backend();
    settings.storage.location = explicit_location(&path);
    let online = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let identity = online.identity();
    let inbox_id = online.inbox_id();
    online
        .inner
        .context
        .db()
        .record_server_configuration_conflict("other-deployment")?;
    online.end().await?;

    relay.refuse();
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
    assert_eq!(relay.connections(), 0, "the build sent a request");
}
