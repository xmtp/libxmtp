use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use xmtp_configuration::CUTOVER_REFRESH_TIME;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api::Client;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::prelude::*;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::types::{GroupId, GroupMessage};

use crate::D14nClient;
use crate::ToDynApi;
use crate::V3Client;
use crate::d14n::FetchD14nCutover;
use crate::definitions::XmtpApiClient;
use crate::protocol::CursorStore;

mod connected_check;
mod streams;
mod to_dyn_api;
mod xmtp_query;

static ERROR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(xmtp_configuration::D14N_MIGRATION_MSG_REGEX).expect("static regex must be valid")
});

#[derive(Clone)]
pub struct MigrationClient<V3, D14n, Store> {
    pub(crate) v3_client: XmtpApiClient,
    pub(crate) xmtpd_client: XmtpApiClient,
    store: Store,
    v3_grpc: V3,
    xmtpd_grpc: D14n,
    always_check_once: OnceLock<()>,
}

impl<V3, D14n, Store> MigrationClient<V3, D14n, Store>
where
    V3: Client + IsConnectedCheck + Clone + 'static,
    D14n: Client + IsConnectedCheck + Clone + 'static,
    Store: CursorStore + Clone + 'static,
{
    pub fn new(v3: V3, d14n: D14n, store: Store) -> Result<Self, VerifierError> {
        Ok(Self {
            v3_grpc: v3.clone(),
            xmtpd_grpc: d14n.clone(),
            store: store.clone(),
            v3_client: V3Client::new(v3, store.clone()).arced(),
            xmtpd_client: D14nClient::new(d14n, store)?.arced(),
            always_check_once: OnceLock::new(),
        })
    }
}

impl<V3, D14n, S> MigrationClient<V3, D14n, S>
where
    V3: Client,
    D14n: Client,
    S: CursorStore,
{
    pub async fn choose_client(&self) -> Result<&XmtpApiClient, ApiClientError> {
        if self.store.has_migrated()? {
            return Ok(&self.xmtpd_client);
        }

        let now = xmtp_common::time::now_ns();
        let cutover_ns = self.store.get_cutover_ns()?;
        let last_checked = self.store.get_last_checked_ns()?;
        let time_since_refresh = now.saturating_sub(last_checked);
        let cutover_ns = if time_since_refresh >= CUTOVER_REFRESH_TIME
            || self.always_check_once.set(()).is_ok()
        {
            self.refresh_cutover().await?
        } else {
            cutover_ns
        };

        if now >= cutover_ns {
            self.store.set_has_migrated(true)?;
            Ok(&self.xmtpd_client)
        } else {
            Ok(&self.v3_client)
        }
    }

    async fn refresh_cutover(&self) -> Result<i64, ApiClientError> {
        let cutover_ns = FetchD14nCutover.query(&self.v3_grpc).await?.timestamp_ns as i64;
        self.store.set_cutover_ns(cutover_ns)?;
        self.store
            .set_last_checked_ns(xmtp_common::time::now_ns())?;
        Ok(cutover_ns)
    }

    /// if the write fails because of a cutover, force a refresh and retry
    pub async fn write_with_refresh<F, R, Fut>(&self, f: F) -> Result<R, ApiClientError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<R, ApiClientError>>,
    {
        let out = f().await;
        if let Err(ref e) = out {
            if let Some(network) = e.network_error() {
                let s = network.to_string();
                if ERROR_REGEX.is_match(&s) {
                    self.refresh_cutover().await?;
                    return f().await;
                }
            }
        }
        out
    }
}

#[xmtp_common::async_trait]
impl<V3, D14n, S> XmtpMlsClient for MigrationClient<V3, D14n, S>
where
    V3: Client,
    D14n: Client,
    S: CursorStore,
{
    type Error = ApiClientError;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .upload_key_package(request)
            .await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        self.choose_client()
            .await?
            .fetch_key_packages(request)
            .await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.write_with_refresh(|| {
            let value = request.clone();
            async move {
                self.choose_client()
                    .await?
                    .send_group_messages(value.clone())
                    .await
            }
        })
        .await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.choose_client()
            .await?
            .send_welcome_messages(request)
            .await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<GroupMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_group_messages(group_id)
            .await
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_latest_group_message(group_id)
            .await
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        self.choose_client()
            .await?
            .query_welcome_messages(installation_key)
            .await
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        self.write_with_refresh(|| {
            let value = request.clone();
            async move {
                self.choose_client()
                    .await?
                    .publish_commit_log(value.clone())
                    .await
            }
        })
        .await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        self.choose_client().await?.query_commit_log(request).await
    }

    async fn get_newest_group_message(
        &self,
        request: mls_v1::GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessageMetadata>>, Self::Error> {
        self.choose_client()
            .await?
            .get_newest_group_message(request)
            .await
    }
}

#[xmtp_common::async_trait]
impl<V3, D14n, S> XmtpIdentityClient for MigrationClient<V3, D14n, S>
where
    S: CursorStore,
    V3: Client,
    D14n: Client,
{
    type Error = ApiClientError;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        self.write_with_refresh(|| {
            let value = request.clone();
            async move {
                self.choose_client()
                    .await?
                    .publish_identity_update(value.clone())
                    .await
            }
        })
        .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        self.choose_client()
            .await?
            .get_identity_updates_v2(request)
            .await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        self.choose_client().await?.get_inbox_ids(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.choose_client()
            .await?
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn regex_does_not_panic() {
        assert!(!*ERROR_REGEX.is_match("hi"))
    }
}
