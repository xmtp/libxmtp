mod cached;
pub use cached::CachedSmartContractSignatureVerifier;
mod chain_rpc_verifier;
mod remote_signature_verifier;
use crate::associations::AccountId;
use alloy::{
    primitives::{BlockNumber, Bytes},
    providers::DynProvider,
};
pub use chain_rpc_verifier::*;
pub use remote_signature_verifier::*;
use std::{collections::HashMap, fs, path::Path, sync::Arc};
use thiserror::Error;
use tracing::info;
use url::Url;
use xmtp_common::{ErrorCode, MaybeSend, MaybeSync, RetryableError};

static DEFAULT_CHAIN_URLS: &str = include_str!("chain_urls_default.json");

#[derive(Debug, Error, ErrorCode)]
pub enum VerifierError {
    /// Unexpected ERC-6492 result.
    ///
    /// Smart contract wallet signature verification returned unexpected result. Not retryable.
    #[error("unexpected result from ERC-6492 {0}")]
    UnexpectedERC6492Result(String),
    #[error(transparent)]
    #[error_code(inherit)]
    FromHex(#[from] hex::FromHexError),
    /// Provider error.
    ///
    /// Ethereum RPC provider error. Retryable.
    #[error(transparent)]
    Provider(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
    /// URL parse error.
    ///
    /// Verifier URL is malformed. Not retryable.
    #[error(transparent)]
    Url(#[from] url::ParseError),
    /// I/O error.
    ///
    /// I/O operation failed. May be retryable.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Serialization error.
    ///
    /// JSON serialization/deserialization failed. Not retryable.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Malformed chain ID.
    ///
    /// Chain ID string lacks expected eip155: prefix. Not retryable.
    #[error("Chain IDs must be preceded with eip155:")]
    MalformedEipUrl,
    /// No verifier.
    ///
    /// Verifier not configured for the given chain ID. Retryable.
    #[error("verifier not present for chain ID {0}")]
    NoVerifier(String),
    /// Missing block.
    ///
    /// The chain did not return a block at or below its reported head. Retryable.
    #[error("chain did not return block {0}")]
    MissingBlock(BlockNumber),
    /// Invalid hash.
    ///
    /// Hash has invalid length or format. Not retryable.
    #[error("hash was invalid length or otherwise malformed")]
    InvalidHash(Vec<u8>),
    /// Other error.
    ///
    /// Unclassified verifier error. May be retryable.
    #[error("{0}")]
    Other(Box<dyn RetryableError>),
}

impl RetryableError for VerifierError {
    fn is_retryable(&self) -> bool {
        use VerifierError::*;
        match self {
            Io(_) => true,
            NoVerifier(_) => true,
            MissingBlock(_) => true,
            Provider(_) => true,
            Other(o) => o.is_retryable(),
            _ => false,
        }
    }
}

#[xmtp_common::async_trait]
pub trait SmartContractSignatureVerifier: MaybeSend + MaybeSync {
    /// Verifies an ERC-6492<https://eips.ethereum.org/EIPS/eip-6492> signature.
    ///
    /// # Arguments
    ///
    /// * `signer` - can be the smart wallet address or EOA address.
    /// * `hash` - Message digest for the signature.
    /// * `signature` - Could be encoded smart wallet signature or raw ECDSA signature.
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError>;
}

#[xmtp_common::async_trait]
impl<T> SmartContractSignatureVerifier for Arc<T>
where
    T: SmartContractSignatureVerifier,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        (**self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

#[xmtp_common::async_trait]
impl<T> SmartContractSignatureVerifier for &T
where
    T: SmartContractSignatureVerifier,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        (*self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

#[xmtp_common::async_trait]
impl<T> SmartContractSignatureVerifier for Box<T>
where
    T: SmartContractSignatureVerifier + ?Sized,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        (**self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

/// A block's number and timestamp, in seconds, as a chain reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockStamp {
    pub number: BlockNumber,
    pub timestamp: u64,
}

/// Read access to chain blocks, keyed by `eip155:<chain>` identifier.
///
/// Admission uses it to judge how fresh a signature's stated block is. An
/// unrouted chain or failed call is a retryable [`VerifierError`], never a
/// verdict on the signature.
#[xmtp_common::async_trait]
pub trait ChainBlocks: MaybeSend + MaybeSync {
    /// The chain's head block.
    async fn head(&self, chain_id: &str) -> Result<BlockStamp, VerifierError>;
    /// The timestamp of block `number`, which the caller has seen at or below the head.
    async fn timestamp(&self, chain_id: &str, number: BlockNumber) -> Result<u64, VerifierError>;
}

#[derive(Clone)]
/// Result of one smart-contract-wallet signature check.
///
/// A negative verdict is a successful check and is represented by
/// `is_valid = false`. Provider failures use `VerifierError` instead.
pub struct ValidationResponse {
    /// Whether the signature is valid for the requested account and block.
    pub is_valid: bool,
    /// The block used by the provider, when it can report one.
    pub block_number: Option<u64>,
    /// Provider detail for a negative verdict, when available.
    pub error: Option<String>,
}

/// Routes signature checks to a verifier selected by chain ID.
///
/// Each configured key is an `eip155:<chain>` identifier. A missing route is a
/// retryable configuration/provider error so callers can distinguish it from a
/// bad signature.
pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, RpcSmartContractWalletVerifier>,
}

impl std::fmt::Debug for MultiSmartContractSignatureVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSmartContractSignatureVerifier")
            .field("verifiers", &self.verifiers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl MultiSmartContractSignatureVerifier {
    /// Build RPC verifiers from chain IDs and endpoint URLs.
    ///
    /// URL parsing and provider construction errors are returned before a
    /// partially configured verifier is created.
    pub fn new(urls: HashMap<String, url::Url>) -> Result<Self, VerifierError> {
        let verifiers = urls
            .into_iter()
            .map(|(chain_id, url)| {
                Ok::<_, VerifierError>((
                    chain_id,
                    RpcSmartContractWalletVerifier::new(url.to_string())?,
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(Self { verifiers })
    }

    /// Build RPC verifiers from already-created providers.
    ///
    /// The provider map is consumed and keyed by the caller's chain IDs.
    pub fn new_providers(providers: HashMap<String, DynProvider>) -> Result<Self, VerifierError> {
        let verifiers = providers
            .into_iter()
            .map(|(chain_id, provider)| {
                (
                    chain_id,
                    RpcSmartContractWalletVerifier::new_from_provider(provider),
                )
            })
            .collect();
        Ok(Self { verifiers })
    }

    /// Load the default chain routes, apply environment overrides, and add Anvil.
    pub fn new_from_env() -> Result<Self, VerifierError> {
        let urls: HashMap<String, Url> = serde_json::from_str(DEFAULT_CHAIN_URLS)?;
        Self::new(urls)?.upgrade()
    }

    /// Load chain routes from a JSON file.
    ///
    /// The file must contain a map from chain ID to URL. Environment upgrades
    /// are not applied by this constructor.
    pub fn new_from_file(path: impl AsRef<Path>) -> Result<Self, VerifierError> {
        let json = fs::read_to_string(path.as_ref())?;
        let urls: HashMap<String, Url> = serde_json::from_str(&json)?;

        Self::new(urls)
    }

    /// Replace default routes with environment overrides when present.
    ///
    /// This also registers the configured Anvil endpoint. A malformed chain ID
    /// or endpoint prevents the verifier from being returned.
    pub fn upgrade(mut self) -> Result<Self, VerifierError> {
        for (id, verifier) in self.verifiers.iter_mut() {
            // TODO: coda - update the chain id env var ids to preceded with "EIP155_"
            let eip_id = id.split(":").nth(1).ok_or(VerifierError::MalformedEipUrl)?;
            if let Ok(url) = std::env::var(format!("CHAIN_RPC_{eip_id}")) {
                *verifier = RpcSmartContractWalletVerifier::new(url)?;
            } else {
                info!("No upgraded chain url for chain {id}, using default.");
            };
        }

        if let Ok(url) = std::env::var("ANVIL_URL") {
            info!("Adding anvil from env to the verifiers: {url}");
            self.add_anvil(url)?;
        } else {
            use xmtp_configuration::DockerUrls;
            let url = DockerUrls::anvil();
            info!("adding default anvil url @{url}");
            self.add_anvil(url)?;
        }
        Ok(self)
    }

    /// The verifier routed for `chain_id`, or a retryable error when none is.
    fn route(&self, chain_id: &str) -> Result<&RpcSmartContractWalletVerifier, VerifierError> {
        self.verifiers
            .get(chain_id)
            .ok_or_else(|| VerifierError::NoVerifier(chain_id.to_string()))
    }

    /// Add or replace one chain verifier backed by an RPC URL.
    pub fn add_verifier(&mut self, id: String, url: String) -> Result<(), VerifierError> {
        self.verifiers
            .insert(id, RpcSmartContractWalletVerifier::new(url)?);
        Ok(())
    }

    /// Add or replace the local Anvil verifier route.
    pub fn add_anvil(&mut self, url: String) -> Result<(), VerifierError> {
        self.verifiers.insert(
            "eip155:31337".to_string(),
            RpcSmartContractWalletVerifier::new(url)?,
        );
        Ok(())
    }
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for MultiSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        self.route(&account_id.chain_id)?
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

#[xmtp_common::async_trait]
impl ChainBlocks for MultiSmartContractSignatureVerifier {
    async fn head(&self, chain_id: &str) -> Result<BlockStamp, VerifierError> {
        self.route(chain_id)?.head(chain_id).await
    }

    async fn timestamp(&self, chain_id: &str, number: BlockNumber) -> Result<u64, VerifierError> {
        self.route(chain_id)?.timestamp(chain_id, number).await
    }
}
