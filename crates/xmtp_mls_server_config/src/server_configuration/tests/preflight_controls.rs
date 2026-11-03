//! Resolution leaves no configuration check pending when the stored URL
//! already matches, when the client has no URL to compare, and refuses a
//! recorded conflict.

use super::*;
use crate::server_configuration::test_support::scripted_api::*;
use parking_lot::Mutex;
use prost::Message;
use std::sync::Arc;
use xmtp_db::{TestDb, XmtpTestDb};

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_resolution_preserves_ready_and_conflict_controls() {
    for stored_url in [None, Some(NEW), Some("http://new.example///")] {
        let db = TestDb::create_ephemeral_store().await;
        if let Some(url) = stored_url {
            db.db().store_server_configuration(
                IDENTIFIER,
                url,
                &response(IDENTIFIER).encode_to_vec(),
                1,
            )?;
        }
        let script = Arc::new(Script {
            db: db.clone(),
            calls: Mutex::default(),
            responses: Mutex::default(),
            pause: Mutex::default(),
            wire: Mutex::default(),
        });
        let api = xmtp_api::ApiClientWrapper::new(
            ScriptedApi(script.clone()),
            xmtp_common::Retry::default(),
        );
        let handle = crate::server_configuration::resolve(&api, &db.db(), true).await?;
        assert!(!handle.requires_preflight());
        assert!(script.calls.lock().is_empty());
    }
    let db = TestDb::create_ephemeral_store().await;
    db.db().store_server_configuration(
        IDENTIFIER,
        OLD,
        &response(IDENTIFIER).encode_to_vec(),
        1,
    )?;
    // This mock has no URL metadata and no network expectations.
    let api = xmtp_api::ApiClientWrapper::new(
        xmtp_api_backend::MockBackendClient::new(),
        xmtp_common::Retry::default(),
    );
    let handle = crate::server_configuration::resolve(&api, &db.db(), true).await?;
    assert!(!handle.requires_preflight());
    db.db()
        .record_server_configuration_conflict("org.example.other")?;
    let result = crate::server_configuration::resolve(&api, &db.db(), true).await;
    assert!(
        matches!(result, Err(ServerConfigError::Blocked(BlockedConnection::BackendMismatch { stored, received })) if stored == IDENTIFIER && received == "org.example.other")
    );
}
