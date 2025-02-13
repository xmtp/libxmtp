use crate::Client;
use xmtp_proto::{
    api_client::XmtpIdentityClient,
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
    ApiEndpoint,
};

#[async_trait::async_trait]
impl XmtpIdentityClient for Client {
    type Error = crate::Error;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        let client = &mut self.identity_client.clone();

        client
            .publish_identity_update(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| crate::Error::new(ApiEndpoint::PublishIdentityUpdate, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        let client = &mut self.identity_client.clone();

        client
            .get_inbox_ids(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| crate::Error::new(ApiEndpoint::GetInboxIds, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        let client = &mut self.identity_client.clone();

        client
            .get_identity_updates(self.build_request(request))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| crate::Error::new(ApiEndpoint::GetIdentityUpdatesV2, e.into()))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        let client = &mut self.identity_client.clone();

        let res = client
            .verify_smart_contract_wallet_signatures(self.build_request(request))
            .await;

        res.map(|response| response.into_inner())
            .map_err(|err| crate::Error::new(ApiEndpoint::VerifyScwSignature, err.into()))
    }
}
