use crate::Client;
use xmtp_proto::{
    ApiEndpoint,
    api_client::{IdentityStats, XmtpIdentityClient},
    traits::ApiClientError,
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
};

#[async_trait::async_trait]
impl XmtpIdentityClient for Client {
    type Error = ApiClientError<crate::GrpcError>;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        self.identity_stats.publish_identity_update.count_request();

        let client = &mut self.identity_client.clone();

        client
            .publish_identity_update(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::PublishIdentityUpdate, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        self.identity_stats.get_inbox_ids.count_request();

        let client = &mut self.identity_client.clone();

        client
            .get_inbox_ids(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::GetInboxIds, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        self.identity_stats.get_identity_updates_v2.count_request();

        let client = &mut self.identity_client.clone();

        client
            .get_identity_updates(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| ApiClientError::new(ApiEndpoint::GetIdentityUpdatesV2, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.identity_stats
            .verify_smart_contract_wallet_signature
            .count_request();

        let client = &mut self.identity_client.clone();

        let res = client
            .verify_smart_contract_wallet_signatures(self.build_request(request))
            .await;

        res.map(|response| response.into_inner())
            .map_err(|err| ApiClientError::new(ApiEndpoint::VerifyScwSignature, err.into()))
    }

    fn identity_stats(&self) -> IdentityStats {
        self.identity_stats.clone()
    }
}
