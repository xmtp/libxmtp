use xmtp_proto::{
    api_client::{Error, ErrorKind, XmtpIdentityClient},
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    },
};
use xmtp_proto::xmtp::xmtpv4::ClientEnvelope;
use crate::Client;
use crate::conversions::wrap_client_envelope;

impl XmtpIdentityClient for Client {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        if self.use_replication_v4 {
            let client = &mut self.replication_client.clone();
            let payload = wrap_client_envelope(ClientEnvelope::from(request));
            let res = client.publish_envelope(payload).await;
            match res {
                Ok(_) => Ok(PublishIdentityUpdateResponse{}),
                Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
            }
        } else {
            let client = &mut self.identity_client.clone();

            let res = client
                .publish_identity_update(self.build_request(request))
                .await;

            res.map(|response| response.into_inner())
                .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
        }
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
}
