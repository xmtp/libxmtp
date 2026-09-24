use std::sync::Arc;

use xmtp_mls::context::XmtpSharedContext;
use xmtp_proto::api::HasStats;

use crate::{XmtpError, client::CoreClient, conversation::on_sdk_worker};

#[derive(Clone, Debug, uniffi::Record)]
pub struct ApiStats {
    pub publish: u64,
    pub query: u64,
    pub query_newest: u64,
    pub subscribe: u64,
    pub subscribe_static: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct IdentityStats {
    pub get_inbox_ids: u64,
    pub verify_smart_contract_wallet_signatures: u64,
}

#[derive(uniffi::Object)]
pub struct Diagnostics {
    pub(crate) client: Arc<CoreClient>,
}

#[xmtp_macro::sdk_export]
impl Diagnostics {
    pub async fn api_statistics(&self) -> Result<ApiStats, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let stats = client.context.api().api_client.mls_stats();
            Ok(ApiStats {
                publish: stats.publish.get_count() as u64,
                query: stats.query.get_count() as u64,
                query_newest: stats.query_newest.get_count() as u64,
                subscribe: stats.subscribe.get_count() as u64,
                subscribe_static: stats.subscribe_static.get_count() as u64,
            })
        })
        .await
    }

    pub async fn identity_statistics(&self) -> Result<IdentityStats, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let stats = client.context.api().api_client.identity_stats();
            Ok(IdentityStats {
                get_inbox_ids: stats.get_inbox_ids.get_count() as u64,
                verify_smart_contract_wallet_signatures: stats
                    .verify_smart_contract_wallet_signatures
                    .get_count() as u64,
            })
        })
        .await
    }

    pub async fn aggregate_statistics(&self) -> Result<String, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            Ok(format!(
                "{:?}",
                (
                    client.context.api().api_client.mls_stats(),
                    client.context.api().api_client.identity_stats()
                )
            ))
        })
        .await
    }

    pub async fn clear_statistics(&self) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.context.api().api_client.mls_stats().clear();
            client.context.api().api_client.identity_stats().clear();
            Ok(())
        })
        .await
    }
}
