use ethers::{
    prelude::LocalWallet,
    prelude::Provider,
    providers::{ProviderError, Ws},
    signers::Signer,
    types::{Address, Bytes, ParseBytesError, U256},
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
use xmtp_mls::credential::{AssociationError, Credential as XmtpCredential};
use xmtp_proto::xmtp::mls::{
    api::v1::{
        get_identity_updates_response::{
            update::Kind as UpdateKind, NewInstallationUpdate, Update, WalletUpdates,
        },
        GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, RegisterInstallationRequest,
        RegisterInstallationResponse,
    },
    message_contents::MlsCredential as MlsCredentialProto,
};
use xps_types::{InstallationId, DID_ETH_REGISTRY};
// pub const DID_ETH_REGISTRY: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

#[derive(Debug, Error)]
pub enum XpsClientError {
    #[error("XPS client error: {0}")]
    XmtpError(#[from] xmtp_proto::api_client::Error),
    #[error("RPC Error interacting with XPS JSON-RPC: {0}")]
    JsonRpc(#[from] jsonrpsee::core::ClientError),
    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Address error: {0}")]
    RustcHex(#[from] rustc_hex::FromHexError),
    #[error("Hex decode error {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Registry Error {0}")]
    Registry(#[from] lib_didethresolver::error::RegistrySignerError<Provider<Ws>>),
    #[error("Error unraveling key association credential {0}")]
    Credential(#[from] AssociationError),
    #[error("Error connecting to ethereum {0}")]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    IntTypeError(#[from] std::num::TryFromIntError),
    #[error("Error parsing bytes {0}")]
    Bytes(#[from] ParseBytesError),
}

pub struct XpsOperations {
    /// Client for the XPS API
    client: WsClient,
    /// Local Wallet for signing
    owner: LocalWallet,
    // TODO: Should not be needed to interact directly with DID Registry. This is used for:
    // - getting nonce for payload signing
    registry: DIDRegistry<Provider<Ws>>,
}

impl XpsOperations {
    pub async fn new<S: AsRef<str>, P: AsRef<str>>(
        endpoint: S,
        owner: LocalWallet,
        network_endpoint: P,
    ) -> Result<Self, XpsClientError> {
        let client = WsClientBuilder::default().build(endpoint.as_ref()).await?;
        log::info!("Connected to XPS at {}", endpoint.as_ref());
        // TODO: we need to provide a nonce endpoint and modify the signing functions as needed to avoid a
        // dependency on Ethereum RPC in libxmtp
        let provider = Provider::connect(network_endpoint.as_ref()).await?;
        let contract = Address::from_str(DID_ETH_REGISTRY)?;
        let registry = DIDRegistry::new(contract, provider.into());
        log::info!("Connected to ethereum at {}", network_endpoint.as_ref());

        Ok(Self {
            client,
            owner,
            registry,
        })
    }

    pub async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
        expected_id: Vec<u8>,
    ) -> Result<RegisterInstallationResponse, XpsClientError> {
        log::info!("Registering installation");
        let kp_bytes = request.key_package.unwrap().key_package_tls_serialized;

        let attribute = XmtpAttribute {
            purpose: XmtpKeyPurpose::Installation,
            encoding: KeyEncoding::Hex,
        };
        log::debug!("Expected installation_id: {:?}", expected_id);
        // sign the attribute with the owner's wallet
        // `InboxOwner` should require an implementation of `RegistrySignerExt`
        // If signing is done in the SDK's, then the SDKS need to send the signed payload to the
        // wasm blob or through the bindings
        let signature = self
            .owner
            .sign_attribute(
                &self.registry,
                attribute.clone().into(),
                expected_id,
                U256::from(DEFAULT_ATTRIBUTE_VALIDITY),
            )
            .await?;

        let result = self
            .client
            .grant_installation(
                format!("0x{}", hex::encode(self.owner.address())),
                attribute,
                kp_bytes,
                signature,
            )
            .await?;

        log::info!("Grant Installation Result: {:?}", result);

        Ok(RegisterInstallationResponse {
            installation_key: Vec::new(),
        })
    }

    pub async fn get_identity_updates(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, XpsClientError> {
        let mut updates = Vec::new();
        log::info!("Fetching key packages {:?}", request);
        for address in request.account_addresses.iter() {
            let address = Bytes::from_str(address)?;
            let keys = self
                .client
                .fetch_key_packages(address.to_string(), request.start_time_ns.try_into()?)
                .await?
                .installations
                .into_iter()
                .map(|InstallationId { timestamp_ns, id }| {
                    Ok::<_, XpsClientError>(Update {
                        timestamp_ns,
                        kind: Some(UpdateKind::NewInstallation(NewInstallationUpdate {
                            installation_key: id,
                            credential_identity: address.to_vec(),
                        })),
                    })
                })
                .collect::<Result<Vec<_>, _>>();

            updates.push(WalletUpdates { updates: keys? });
        }
        log::info!("Updates: {:?}", updates);

        Ok(GetIdentityUpdatesResponse { updates })
    }
}
