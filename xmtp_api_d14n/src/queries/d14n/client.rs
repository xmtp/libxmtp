use std::sync::Arc;

use xmtp_common::RetryableError;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::types::AppVersion;

#[derive(Clone)]
pub struct D14nClient<C, Store> {
    pub(super) client: C,
    pub(super) cursor_store: Store,
    pub(super) scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
}

impl<C, Store> D14nClient<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Result<Self, VerifierError> {
        Ok(Self {
            client,
            cursor_store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, Store> IsConnectedCheck for D14nClient<C, Store>
where
    C: IsConnectedCheck + Send + Sync,
    Store: Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await && self.client.is_connected().await
    }
}

pub struct D14nClientBuilder<Builder, Store> {
    client: Builder,
    store: Store,
}

impl<Builder, Store> D14nClientBuilder<Builder, Store> {
    pub fn new(client: Builder, store: Store) -> Self {
        Self { client, store }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum D14nBuilderError<E> {
    #[error(transparent)]
    ClientError(E),
    #[error(transparent)]
    VerifierError(#[from] VerifierError),
}

impl<E: RetryableError> RetryableError for D14nBuilderError<E> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ClientError(e) => e.is_retryable(),
            Self::VerifierError(v) => v.is_retryable(),
        }
    }
}

// TODO: should deprecate these api builders
// it is not a good solution for the variety o clients we have now
// its OK for low-level GRPC clients
impl<Builder, Store> ApiBuilder for D14nClientBuilder<Builder, Store>
where
    Builder: ApiBuilder,
{
    type Output = D14nClient<<Builder as ApiBuilder>::Output, Store>;
    type Error = D14nBuilderError<<Builder as ApiBuilder>::Error>;
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_libxmtp_version(&mut self.client, version)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_app_version(&mut self.client, version)
    }

    fn set_host(&mut self, host: String) {
        unimplemented!()
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder as ApiBuilder>::set_tls(&mut self.client, tls)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder as ApiBuilder>::rate_per_minute(&mut self.client, limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        <Builder as ApiBuilder>::port(&self.client)
    }

    fn host(&self) -> Option<&str> {
        <Builder as ApiBuilder>::host(&self.client)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient {
            client: <Builder as ApiBuilder>::build(self.client)?,
            cursor_store: self.store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder as ApiBuilder>::set_retry(&mut self.client, retry)
    }
}
