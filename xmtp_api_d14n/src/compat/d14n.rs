//! Compatibility layer for d14n and previous xmtp_api crate

//TODO: Remove once d14n integration complete
#![allow(unused)]

use crate::d14n::QueryEnvelope;
use crate::{d14n::PublishClientEnvelopes, d14n::QueryEnvelopes, endpoints::d14n::GetInboxIds};
use std::marker::PhantomData;
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{
    ApiStats, IdentityStats, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams,
};
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiClientError, Query};
use xmtp_proto::v4_utils::{
    build_group_message_topic, build_identity_topic_from_hex_encoded, build_key_package_topic,
    build_welcome_message_topic, Extract,
};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::{
    IdentityUpdateLog, Response,
};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, GetInboxIdsResponse as GetInboxIdsResponseV4, QueryEnvelopesResponse,
};
use xmtp_proto::{identity_v1, mls_v1};
use xmtp_proto::{ConversionError, XmtpApiError};

const DEFAULT_PAGINATION_LIMIT: u32 = 100;

pub struct D14nClient<C, P> {
    message_client: C,
    payer_client: P,
}

impl<C, P> D14nClient<C, P> {
    pub fn new(message_client: C, payer_client: P) -> Self {
        Self {
            message_client,
            payer_client,
        }
    }
}

pub struct D14nClientBuilder<Builder1, Builder2> {
    message_client: Builder1,
    payer_client: Builder2,
}

impl<Builder1, Builder2> D14nClientBuilder<Builder1, Builder2> {
    pub fn new(message_client: Builder1, payer_client: Builder2) -> Self {
        Self {
            message_client,
            payer_client,
        }
    }
}

impl<Builder1, Builder2> ApiBuilder for D14nClientBuilder<Builder1, Builder2>
where
    Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
    Builder2: ApiBuilder,
    <Builder1 as ApiBuilder>::Output: xmtp_proto::traits::Client,
    <Builder2 as ApiBuilder>::Output: xmtp_proto::traits::Client,
{
    type Output = D14nClient<<Builder1 as ApiBuilder>::Output, <Builder2 as ApiBuilder>::Output>;

    type Error = <Builder1 as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_libxmtp_version(&mut self.message_client, version.clone());
        <Builder2 as ApiBuilder>::set_libxmtp_version(&mut self.payer_client, version)
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_app_version(&mut self.message_client, version.clone());
        <Builder2 as ApiBuilder>::set_app_version(&mut self.payer_client, version)
    }

    // TODO: Add a builder method for the payer host
    fn set_host(&mut self, host: String) {
        <Builder1 as ApiBuilder>::set_host(&mut self.message_client, host.clone());
        <Builder2 as ApiBuilder>::set_host(&mut self.payer_client, host)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder1 as ApiBuilder>::set_tls(&mut self.message_client, tls);
        <Builder2 as ApiBuilder>::set_tls(&mut self.payer_client, tls)
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient::new(
            <Builder1 as ApiBuilder>::build(self.message_client).await?,
            <Builder2 as ApiBuilder>::build(self.payer_client).await?,
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpMlsClient for D14nClient<C, P>
where
    E: XmtpApiError + std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<P as Client>::Error>>
        + From<ApiClientError<<C as Client>::Error>>
        + Send
        + Sync
        + 'static,
{
    type Error = ApiClientError<E>;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let envelope: ClientEnvelope = request.try_into()?;

        PublishClientEnvelopes::builder()
            .envelopes(vec![envelope])
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(())
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        let topics = request
            .installation_keys
            .iter()
            .map(|key| build_key_package_topic(key))
            .collect();

        let result: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topics(topics)
            .build()?
            .query(&self.message_client)
            .await?;

        let key_packages = result
            .envelopes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(mls_v1::FetchKeyPackagesResponse { key_packages })
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelopes: Vec<ClientEnvelope> = request
            .messages
            .into_iter()
            .map(|message| message.try_into())
            .collect::<Result<_, _>>()?;

        PublishClientEnvelopes::builder()
            .envelopes(envelopes)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(())
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelope: Vec<ClientEnvelope> = request
            .messages
            .into_iter()
            .map(|message| message.try_into())
            .collect::<Result<_, _>>()?;

        PublishClientEnvelopes::builder()
            .envelopes(envelope)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(())
    }

    async fn query_group_messages(
        &self,
        request: mls_v1::QueryGroupMessagesRequest,
    ) -> Result<mls_v1::QueryGroupMessagesResponse, Self::Error> {
        let response: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topic(build_group_message_topic(request.group_id.as_slice()))
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = response
            .envelopes
            .into_iter()
            .map(mls_v1::GroupMessage::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(mls_v1::QueryGroupMessagesResponse {
            messages,
            paging_info: None,
        })
    }

    async fn query_welcome_messages(
        &self,
        request: mls_v1::QueryWelcomeMessagesRequest,
    ) -> Result<mls_v1::QueryWelcomeMessagesResponse, Self::Error> {
        let response = QueryEnvelope::builder()
            .topic(build_welcome_message_topic(
                request.installation_key.as_slice(),
            ))
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = response
            .envelopes
            .into_iter()
            .map(mls_v1::WelcomeMessage::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(mls_v1::QueryWelcomeMessagesResponse {
            messages,
            paging_info: None,
        })
    }

    fn stats(&self) -> ApiStats {
        Default::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpIdentityClient for D14nClient<C, P>
where
    E: XmtpApiError + std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client<Error = E>,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<P as Client>::Error>>
        + From<ApiClientError<<C as Client>::Error>>
        + Send
        + Sync
        + 'static,
{
    type Error = ApiClientError<E>;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        let envelope: ClientEnvelope = request.try_into().map_err(ApiClientError::Conversion)?;
        let result = PublishClientEnvelopes::builder()
            .envelope(envelope)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(identity_v1::PublishIdentityUpdateResponse {})
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        let topics = request
            .requests
            .iter()
            .map(|r| build_identity_topic_from_hex_encoded(&r.inbox_id.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        let result: QueryEnvelopesResponse = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: topics.clone(),
                originator_node_ids: vec![], //todo: set later
                last_seen: None,             //todo: set later
            })
            .build()?
            .query(&self.message_client)
            .await?;

        let joined_data: Vec<_> = result
            .envelopes
            .into_iter()
            .zip(request.requests.into_iter())
            .collect();
        let responses: Vec<Response> = joined_data
            .into_iter()
            .map(|(envelopes, inner_req)| {
                let identity_update_log: IdentityUpdateLog = envelopes.try_into()?;
                Ok(Response {
                    inbox_id: inner_req.inbox_id.clone(),
                    updates: vec![identity_update_log],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(identity_v1::GetIdentityUpdatesResponse { responses })
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        let res: GetInboxIdsResponseV4 = GetInboxIds::builder()
            .addresses(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Ethereum as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .passkeys(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Passkey as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .build()?
            .query(&self.message_client)
            .await?;

        Ok(identity_v1::GetInboxIdsResponse {
            responses: res
                .responses
                .iter()
                .map(|r| identity_v1::get_inbox_ids_response::Response {
                    identifier: r.identifier.clone(),
                    identifier_kind: IdentifierKind::Ethereum as i32,
                    inbox_id: r.inbox_id.clone(),
                })
                .collect::<Vec<_>>(),
        })
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        unimplemented!()
    }

    fn identity_stats(&self) -> IdentityStats {
        Default::default()
    }
}
