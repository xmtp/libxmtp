//! A backend that answers configuration requests from a script, records
//! each call and checks that every target call sees the stored URL the
//! configuration check committed.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::oneshot;
use xmtp_db::{TestDb, prelude::*};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

/// The backend URL stored before the configuration check.
pub const OLD: &str = "http://old.example";
/// The backend URL the scripted backend reports.
pub const NEW: &str = "http://new.example";
/// The deployment identifier stored and served by default.
pub const IDENTIFIER: &str = "org.example.same";

/// A configuration response naming the deployment `identifier`.
pub fn response(identifier: &str) -> wire::GetConfigurationResponse {
    wire::GetConfigurationResponse {
        identifier: identifier.into(),
        ..Default::default()
    }
}

/// The scripted backend's database, answers and pause, and the calls it
/// received in order.
pub struct Script {
    pub db: TestDb,
    pub calls: Mutex<Vec<&'static str>>,
    pub responses: Mutex<VecDeque<Result<wire::GetConfigurationResponse, ApiClientError>>>,
    pub pause: Mutex<Option<oneshot::Receiver<()>>>,
    pub wire: Mutex<Option<futures::stream::BoxStream<'static, wire::SubscribeRequest>>>,
}
/// A backend client over a [`Script`].
#[derive(Clone)]
pub struct ScriptedApi(pub Arc<Script>);
#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedApi {
    type Error = ApiClientError;
    fn backend_url(&self) -> Option<&str> {
        Some(NEW)
    }
    async fn get_configuration(
        &self,
        _: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.0.calls.lock().push("configuration");
        let pause = self.0.pause.lock().take();
        if let Some(pause) = pause {
            let _ = pause.await;
        }
        self.0
            .responses
            .lock()
            .pop_front()
            .unwrap_or_else(|| Ok(response(IDENTIFIER)))
    }
    async fn publish(&self, _: wire::PublishRequest) -> Result<wire::PublishResponse, Self::Error> {
        self.0.calls.lock().push("publish");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn query(&self, _: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.0.calls.lock().push("query");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn query_raw_envelopes(
        &self,
        request: wire::QueryRequest,
    ) -> Result<xmtp_proto::types::ReceivedQueryResponse, Self::Error> {
        self.query(request).await.map(Into::into)
    }
    async fn query_newest(
        &self,
        _: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.0.calls.lock().push("query_newest");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn create_upload(
        &self,
        _: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.0.calls.lock().push("create_upload");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn get_inbox_ids(
        &self,
        _: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.0.calls.lock().push("get_inbox_ids");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        _: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.0
            .calls
            .lock()
            .push("verify_smart_contract_wallet_signatures");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn register(
        &self,
        _: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.0.calls.lock().push("register");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn unregister(
        &self,
        _: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.0.calls.lock().push("unregister");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
    async fn update_subscriptions(
        &self,
        _: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.0.calls.lock().push("update_subscriptions");
        assert_eq!(
            self.0
                .db
                .db()
                .server_configuration()
                .expect("a configuration read")
                .expect("a stored configuration")
                .backend_url,
            NEW,
            "target entered before stored URL changed"
        );
        Ok(Default::default())
    }
}

#[xmtp_common::async_trait]
impl xmtp_proto::api_client::XmtpMlsBidiStreams for ScriptedApi {
    type Error = ApiClientError;
    type SubscribeStream = futures::stream::BoxStream<
        'static,
        Result<xmtp_proto::types::ReceivedSubscribeResponse, ApiClientError>,
    >;
    fn host(&self) -> &str {
        NEW
    }
    async fn subscribe_bidi(
        &self,
        requests: futures::stream::BoxStream<'static, wire::SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error> {
        self.0.calls.lock().push("bidi");
        *self.0.wire.lock() = Some(requests);
        Ok(Box::pin(futures::stream::pending()))
    }
}
