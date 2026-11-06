//! An offline client over the scripted configuration backend, built with
//! its deferred configuration check still pending.

use crate::Client;
use crate::identity::IdentityStrategy;
use crate::prelude::*;
use crate::server_configuration::ServerConfigError;
use crate::server_configuration::test_support::scripted_api::{
    IDENTIFIER, OLD, Script, ScriptedApi, response,
};
use parking_lot::Mutex;
use prost::Message;
use std::sync::Arc;
use xmtp_db::{TestDb, XmtpDb, XmtpTestDb, prelude::*};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::{api_client::XmtpBackendClient, backend_v1 as wire};

/// A client over the scripted backend.
pub type TestClient = Client<
    Arc<
        XmtpMlsLocalContext<
            Arc<ScriptedApi>,
            TestDb,
            xmtp_db::sql_key_store::SqlKeyStore<<TestDb as XmtpDb>::DbQuery>,
        >,
    >,
>;
/// The client and its script, without streams or a remote verifier.
pub async fn fixture() -> (TestClient, Arc<Script>) {
    fixture_options(false, false).await
}

pub async fn fixture_options(streams: bool, remote: bool) -> (TestClient, Arc<Script>) {
    let store = TestDb::create_ephemeral_store().await;
    let prior = response(IDENTIFIER);
    store
        .db()
        .store_server_configuration(IDENTIFIER, OLD, &prior.encode_to_vec(), 1)
        .unwrap();
    let script = Arc::new(Script {
        db: store.clone(),
        calls: Mutex::default(),
        responses: Mutex::default(),
        pause: Mutex::default(),
        wire: Mutex::default(),
    });
    let api = Arc::new(ScriptedApi(script.clone()));
    let builder = Client::builder(IdentityStrategy::ExternalIdentity(
        crate::context::test_support::mock_identity(),
    ))
    .store(store);
    let builder = if streams {
        builder.api_client_with_streams(api)
    } else {
        builder.api_client(api)
    };
    let mut builder = builder
        .with_allow_offline(Some(true))
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .default_mls_store()
        .unwrap();
    if remote {
        builder = builder.with_remote_verifier().unwrap();
    }
    let client = builder.build().await.unwrap();
    assert!(
        script.calls.lock().is_empty(),
        "offline build must not dispatch"
    );
    (client, script)
}

pub fn cause<'a>(error: &'a (dyn std::error::Error + 'static)) -> &'a ServerConfigError {
    use std::error::Error;
    xmtp_api::preflight::failure(error)
        .unwrap()
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref()
        .unwrap()
}

pub async fn query(client: &TestClient) -> Result<wire::QueryResponse, xmtp_api::ApiError> {
    client
        .context
        .api()
        .api_client
        .query(Default::default())
        .await
}
