use ethers::{
    prelude::LocalWallet,
    prelude::Provider,
    providers::Http,
    signers::Signer,
    types::{Address, U256},
};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use lib_didethresolver::{
    did_registry::{DIDRegistry, RegistrySignerExt},
    types::{KeyEncoding, XmtpAttribute, XmtpKeyPurpose},
};
use lib_xps::{rpc::DEFAULT_ATTRIBUTE_VALIDITY, XpsClient};
use openmls::prelude::KeyPackageIn;
use prost::Message;
use std::str::FromStr;
use thiserror::Error;
use tls_codec::Deserialize;
use xmtp_mls::credential::Credential as XmtpCredential;
use xmtp_proto::{
    api_client::XmtpMlsClient,
    api_client::{Error as ProtoError, ErrorKind as ProtoErrorKind},
    xmtp::mls::{
        api::v1::{
            FetchKeyPackagesRequest, FetchKeyPackagesResponse, GetIdentityUpdatesRequest,
            GetIdentityUpdatesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
            QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, RegisterInstallationRequest,
            RegisterInstallationResponse, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
            SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
            UploadKeyPackageRequest,
        },
        message_contents::MlsCredential as MlsCredentialProto,
    },
};

pub const DID_ETH_REGISTRY: &str = "0xd1D374DDE031075157fDb64536eF5cC13Ae75000";

#[derive(Debug, Error)]
pub enum XpsClientError {
    #[error("XPS client error: {0}")]
    XmtpError(#[from] xmtp_proto::api_client::Error),
    #[error("RPC Error interacting with XPS JSON-RPC: {0}")]
    JsonRpc(#[from] jsonrpsee::core::ClientError),
    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Address error: {0}")]
    Hex(#[from] rustc_hex::FromHexError),
    #[error("Registry Error {0}")]
    Registry(#[from] lib_didethresolver::error::RegistrySignerError<Provider<Http>>),
}

pub struct XmtpXpsClient<LegacyClient> {
    /// Client for the XPS API
    client: WsClient,
    /// This is the current mls client to fill in non-d14n functionality
    legacy_client: LegacyClient,
    owner: LocalWallet,
    // TODO: Should not be needed to interact directly with DID Registry. This is used for:
    // - getting nonce for payload signing
    registry: DIDRegistry<Provider<Http>>,
}

impl<LegacyClient> XmtpXpsClient<LegacyClient>
where
    LegacyClient: XmtpMlsClient + Send + Sync,
{
    pub async fn new<S: AsRef<str>, P: AsRef<str>>(
        endpoint: S,
        legacy_client: LegacyClient,
        owner: LocalWallet,
        network_endpoint: P,
    ) -> Result<Self, XpsClientError> {
        let client = WsClientBuilder::default()
            .build(&format!("ws://{}", endpoint.as_ref()))
            .await?;

        // TODO: we need to provide a nonce endpoint and modify the signing functions as needed to avoid a
        // dependency on Ethereum RPC in libxmtp
        let provider = Provider::try_from(network_endpoint.as_ref())?;
        let contract = Address::from_str(DID_ETH_REGISTRY)?;
        let registry = DIDRegistry::new(contract, provider.into());

        Ok(Self {
            client,
            legacy_client,
            owner,
            registry,
        })
    }
}

#[async_trait::async_trait]
impl<LegacyClient> XmtpMlsClient for XmtpXpsClient<LegacyClient>
where
    LegacyClient: XmtpMlsClient + Send + Sync,
{
    async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, xmtp_proto::api_client::Error> {
        // NOTE: We are undoing something that was just done right before this call
        // there is a better way
        // this will be OK for now
        let kp_bytes = request.key_package.unwrap().key_package_tls_serialized;
        let kp: KeyPackageIn = Deserialize::tls_deserialize(&mut kp_bytes.as_slice()).unwrap();
        let credential = kp.unverified_credential();
        let credential = credential.credential;

        let credential: MlsCredentialProto = Message::decode(credential.identity()).unwrap();
        let credential = XmtpCredential::from_proto_validated(
            credential,
            Some(&hex::encode(self.owner.address())),
            None,
        )
        .unwrap();

        let attribute = XmtpAttribute {
            purpose: XmtpKeyPurpose::Installation,
            encoding: KeyEncoding::Base64,
        };

        // sign the attribute with the owner's wallet
        // `InboxOwner` should require an implementation of `RegistrySignerExt`
        // If signing is done in the SDK's, then the SDKS need to send the signed payload to the
        // wasm blob or through the bindings
        let signature = self
            .owner
            .sign_attribute(
                &self.registry,
                attribute.clone().into(),
                credential.installation_public_key(),
                U256::from(DEFAULT_ATTRIBUTE_VALIDITY),
            )
            .await
            .map_err(|e| to_client_error(ProtoErrorKind::PublishError, XpsClientError::from(e)))?;

        self.client
            .grant_installation(
                credential.address(),
                attribute,
                credential.installation_public_key(),
                signature,
            )
            .await
            .map_err(|e| to_client_error(ProtoErrorKind::PublishError, XpsClientError::from(e)))?;

        Ok(RegisterInstallationResponse {
            installation_key: Vec::new(),
        })
    }

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.legacy_client.upload_key_package(request).await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, xmtp_proto::api_client::Error> {
        self.legacy_client.fetch_key_packages(request).await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.legacy_client.send_group_messages(request).await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), xmtp_proto::api_client::Error> {
        self.legacy_client.send_welcome_messages(request).await
    }

    async fn get_identity_updates(
        &self,
        _request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, xmtp_proto::api_client::Error> {
        // fetch key packages JSON-RPC (needs a rename)
        unimplemented!()
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, xmtp_proto::api_client::Error> {
        self.legacy_client.query_group_messages(request).await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, xmtp_proto::api_client::Error> {
        self.legacy_client.query_welcome_messages(request).await
    }

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<xmtp_proto::api_client::GroupMessageStream, xmtp_proto::api_client::Error> {
        self.legacy_client.subscribe_group_messages(request).await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<xmtp_proto::api_client::WelcomeMessageStream, xmtp_proto::api_client::Error> {
        self.legacy_client.subscribe_welcome_messages(request).await
    }
}

fn to_client_error<E>(kind: ProtoErrorKind, error: E) -> xmtp_proto::api_client::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    ProtoError::new(kind).with(error)
}
