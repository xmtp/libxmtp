use crate::conversions::wrap_client_envelope;
use crate::Client;
use prost::Message;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::{self, IdentityUpdateLog};
use xmtp_proto::xmtp::xmtpv4::client_envelope::Payload;
use xmtp_proto::xmtp::xmtpv4::envelopes_query::Filter;
use xmtp_proto::xmtp::xmtpv4::{
    ClientEnvelope, EnvelopesQuery, OriginatorEnvelope, QueryEnvelopesRequest,
    UnsignedOriginatorEnvelope,
};
use xmtp_proto::{
    api_client::{Error, ErrorKind, XmtpIdentityClient},
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    },
    xmtp::xmtpv4::GetInboxIdsRequest as GetInboxIdsV4Request,
};

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
                Ok(_) => Ok(PublishIdentityUpdateResponse {}),
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
        let client = &mut self.replication_client.clone();
        let req = GetInboxIdsV4Request {
            requests: request
                .requests
                .into_iter()
                .map(
                    |r| xmtp_proto::xmtp::xmtpv4::get_inbox_ids_request::Request {
                        address: r.address,
                    },
                )
                .collect(),
        };

        let res = client.get_inbox_ids(self.build_request(req)).await;

        res.map(|response| response.into_inner())
            .map(|response| GetInboxIdsResponse {
                responses: response
                    .responses
                    .into_iter()
                    .map(|r| {
                        xmtp_proto::xmtp::identity::api::v1::get_inbox_ids_response::Response {
                            address: r.address,
                            inbox_id: r.inbox_id,
                        }
                    })
                    .collect(),
            })
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        let client = &mut self.replication_client.clone();
        let mut responses = vec![];

        for inner_req in request.requests.iter() {
            let v4_envelopes = client
                .query_envelopes(QueryEnvelopesRequest {
                    query: Some(EnvelopesQuery {
                        last_seen: None,
                        filter: Some(Filter::Topic(build_identity_update_topic(
                            inner_req.inbox_id.clone(),
                        ))),
                    }),
                    limit: 100,
                })
                .await
                .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))?;

            let identity_update_responses = v4_envelopes
                .into_inner()
                .envelopes
                .iter()
                .map(convert_v4_envelope_to_identity_update)
                .collect::<Result<Vec<IdentityUpdateLog>, Error>>()?;

            responses.push(get_identity_updates_response::Response {
                inbox_id: inner_req.inbox_id.clone(),
                updates: identity_update_responses,
            });
        }

        Ok(GetIdentityUpdatesV2Response { responses })
    }
}

fn convert_v4_envelope_to_identity_update(
    envelope: &OriginatorEnvelope,
) -> Result<IdentityUpdateLog, Error> {
    let mut unsigned_originator_envelope = envelope.unsigned_originator_envelope.as_slice();
    let originator_envelope = UnsignedOriginatorEnvelope::decode(&mut unsigned_originator_envelope)
        .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

    let payer_envelope = originator_envelope
        .payer_envelope
        .ok_or(Error::new(ErrorKind::IdentityError).with("Payer envelope is None"))?;

    // TODO: validate payer signatures
    let mut unsigned_client_envelope = payer_envelope.unsigned_client_envelope.as_slice();

    let client_envelope = ClientEnvelope::decode(&mut unsigned_client_envelope)
        .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;
    let payload = client_envelope
        .payload
        .ok_or(Error::new(ErrorKind::IdentityError).with("Payload is None"))?;

    let identity_update = match payload {
        Payload::IdentityUpdate(update) => update,
        _ => {
            return Err(
                Error::new(ErrorKind::IdentityError).with("Payload is not an identity update")
            )
        }
    };

    Ok(IdentityUpdateLog {
        sequence_id: originator_envelope.originator_sequence_id,
        server_timestamp_ns: originator_envelope.originator_ns as u64,
        update: Some(identity_update),
    })
}

fn build_identity_update_topic(inbox_id: String) -> Vec<u8> {
    format!("i/{}", inbox_id).into_bytes()
}
