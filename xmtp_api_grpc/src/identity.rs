use crate::Client;
use xmtp_proto::{
    Error, ErrorKind,
    api_client::{XmtpIdentityClient},
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
};

#[async_trait::async_trait]
impl XmtpIdentityClient for Client {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        let client = &mut self.identity_client.clone();

        let res = client
            .publish_identity_update(self.build_request(request))
            .await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error> {
        let client = &mut self.identity_client.clone();

        let res = client.get_inbox_ids(self.build_request(request)).await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        let client = &mut self.identity_client.clone();

        let res = client
            .get_identity_updates(self.build_request(request))
            .await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error> {
        let client = &mut self.identity_client.clone();

        let res = client
            .verify_smart_contract_wallet_signatures(self.build_request(request))
            .await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }
}
