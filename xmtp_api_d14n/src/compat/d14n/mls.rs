use crate::d14n::PublishClientEnvelopes;
use crate::d14n::QueryEnvelope;
use xmtp_common::RetryableError;
use xmtp_proto::XmtpApiError;
use xmtp_proto::api_client::{ApiStats, XmtpMlsClient};
use xmtp_proto::mls_v1;
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiClientError, Query};
use xmtp_proto::v4_utils::{
    build_group_message_topic, build_key_package_topic, build_welcome_message_topic,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesResponse;

use super::D14nClient;

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
