use crate::{V3Client, v3::*};
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, Query};
use xmtp_proto::api_client::XmtpIdentityClient;
use xmtp_proto::identity_v1;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpIdentityClient for V3Client<C>
where
    C: Client<Error = E>,
    E: RetryableError + 'static,
{
    type Error = ApiClientError<E>;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        PublishIdentityUpdate::builder()
            .identity_update(request.identity_update)
            .build()?
            .query(&self.client)
            .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        GetIdentityUpdatesV2::builder()
            .requests(request.requests)
            .build()?
            .query(&self.client)
            .await
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        GetInboxIds::builder()
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
            .query(&self.client)
            .await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        VerifySmartContractWalletSignatures::builder()
            .signatures(request.signatures)
            .build()?
            .query(&self.client)
            .await
    }
}
