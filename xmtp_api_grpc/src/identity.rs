use tonic::{async_trait, Request};
use xmtp_proto::{
    api_client::{Error, ErrorKind, XmtpIdentityClient},
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    },
};

use crate::Client;

#[async_trait]
impl XmtpIdentityClient for Client {
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        let client = &mut self.identity_client.clone();

        let res = client.publish_identity_update(tonic_request).await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        let client = &mut self.identity_client.clone();

        let res = client.get_inbox_ids(tonic_request).await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        let client = &mut self.identity_client.clone();

        let res = client.get_identity_updates(tonic_request).await;

        res.map(|response| response.into_inner())
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }
}
