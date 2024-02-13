use ethers::{
    prelude::LocalWallet,
    prelude::Provider,
    providers::{ProviderError, Ws},
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
use xps_types::DID_ETH_REGISTRY;
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
    ) -> Result<RegisterInstallationResponse, XpsClientError> {
        log::info!("Registering installation");
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
            Some(&format!("0x{}", hex::encode(self.owner.address()))),
            None,
        )?;

        let attribute = XmtpAttribute {
            purpose: XmtpKeyPurpose::Installation,
            encoding: KeyEncoding::Base64,
        };
        log::info!("Registering with attribute {:?}", attribute);
        log::info!(
            "Registering with value {:?}",
            credential.installation_public_key()
        );

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
            .await?;

        let result = self
            .client
            .grant_installation(
                credential.address(),
                attribute,
                credential.installation_public_key(),
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
        for address in request.account_addresses.iter() {
            let keys = self
                .client
                .fetch_key_packages(address.into())
                .await?
                .installation
                .into_iter()
                .map(|k| {
                    Ok::<_, XpsClientError>(Update {
                        timestamp_ns: 0,
                        kind: Some(UpdateKind::NewInstallation(NewInstallationUpdate {
                            installation_key: k,
                            credential_identity: hex::decode(address)?,
                        })),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            updates.push(WalletUpdates { updates: keys });
        }

        Ok(GetIdentityUpdatesResponse { updates })
    }
}
