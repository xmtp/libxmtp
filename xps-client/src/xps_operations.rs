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
    RustcHex(#[from] rustc_hex::FromHexError),
    #[error("Hex decode error {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Registry Error {0}")]
    Registry(#[from] lib_didethresolver::error::RegistrySignerError<Provider<Http>>),
    #[error("Error unraveling key association credential {0}")]
    Credential(#[from] AssociationError),
}

pub struct XpsOperations {
    /// Client for the XPS API
    client: WsClient,
    /// Local Wallet for signing
    owner: LocalWallet,
    // TODO: Should not be needed to interact directly with DID Registry. This is used for:
    // - getting nonce for payload signing
    registry: DIDRegistry<Provider<Http>>,
}

impl XpsOperations {
    pub async fn new<S: AsRef<str>, P: AsRef<str>>(
        endpoint: S,
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
            owner,
            registry,
        })
    }

    pub async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, XpsClientError> {
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
        )?;

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
            .await?;

        self.client
            .grant_installation(
                credential.address(),
                attribute,
                credential.installation_public_key(),
                signature,
            )
            .await?;

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
