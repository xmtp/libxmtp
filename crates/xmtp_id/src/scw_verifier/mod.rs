mod chain_rpc_verifier;
mod remote_signature_verifier;
use crate::associations::AccountId;
use alloy::{
    primitives::{BlockNumber, Bytes},
    providers::DynProvider,
};
pub use chain_rpc_verifier::*;
pub use remote_signature_verifier::*;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path, sync::Arc};
use thiserror::Error;
use tracing::info;
use url::Url;
use xmtp_common::{ErrorCode, MaybeSend, MaybeSync, RetryableError};

static DEFAULT_CHAIN_URLS: &str = include_str!("chain_urls_default.json");

/// Accepts both a single URL string and an array of URL strings for backward compatibility.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum UrlOrUrls {
    Single(Url),
    Multiple(Vec<Url>),
}

impl UrlOrUrls {
    fn into_vec(self) -> Vec<Url> {
        match self {
            UrlOrUrls::Single(url) => vec![url],
            UrlOrUrls::Multiple(urls) => urls,
        }
    }
}

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
    /// Configuration error.
    ///
    /// Invalid verifier configuration. Not retryable.
    #[error("invalid verifier configuration: {0}")]
    Configuration(String),
    /// No verifier.
    ///
    /// Verifier not configured for the given chain ID. Retryable.
    #[error("verifier not present for chain ID {0}")]
    NoVerifier(String),
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

#[derive(Clone)]
pub struct ValidationResponse {
    pub is_valid: bool,
    pub block_number: Option<u64>,
    pub error: Option<String>,
}

pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, Box<dyn SmartContractSignatureVerifier>>,
}

impl std::fmt::Debug for MultiSmartContractSignatureVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSmartContractSignatureVerifier")
            .field("verifiers", &self.verifiers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Helper to create the appropriate verifier for a list of URLs.
/// Returns a single `RpcSmartContractWalletVerifier` for one URL,
/// or a `FallbackSmartContractWalletVerifier` for multiple.
fn make_verifier(urls: Vec<Url>) -> Result<Box<dyn SmartContractSignatureVerifier>, VerifierError> {
    match urls.len() {
        0 => Err(VerifierError::Configuration(
            "at least one RPC URL is required per chain".into(),
        )),
        1 => Ok(Box::new(RpcSmartContractWalletVerifier::new(
            urls.into_iter().next().unwrap().to_string(),
        )?)),
        _ => Ok(Box::new(FallbackSmartContractWalletVerifier::new(
            urls.into_iter().map(|u| u.to_string()).collect(),
        )?)),
    }
}

impl MultiSmartContractSignatureVerifier {
    pub fn new(urls: HashMap<String, Vec<Url>>) -> Result<Self, VerifierError> {
        let verifiers = urls
            .into_iter()
            .map(|(chain_id, urls)| Ok::<_, VerifierError>((chain_id, make_verifier(urls)?)))
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(Self { verifiers })
    }

    pub fn new_providers(providers: HashMap<String, DynProvider>) -> Result<Self, VerifierError> {
        let verifiers = providers
            .into_iter()
            .map(|(chain_id, provider)| {
                (
                    chain_id,
                    Box::new(RpcSmartContractWalletVerifier::new_from_provider(provider)) as Box<_>,
                )
            })
            .collect();
        Ok(Self { verifiers })
    }

    fn parse_chain_urls(json: &str) -> Result<HashMap<String, Vec<Url>>, VerifierError> {
        let raw: HashMap<String, UrlOrUrls> = serde_json::from_str(json)?;
        Ok(raw.into_iter().map(|(k, v)| (k, v.into_vec())).collect())
    }

    pub fn new_from_env() -> Result<Self, VerifierError> {
        let urls = Self::parse_chain_urls(DEFAULT_CHAIN_URLS)?;
        Self::new(urls)?.upgrade()
    }

    pub fn new_from_file(path: impl AsRef<Path>) -> Result<Self, VerifierError> {
        let json = fs::read_to_string(path.as_ref())?;
        let urls = Self::parse_chain_urls(&json)?;
        Self::new(urls)
    }

    /// Upgrade the default urls to paid/private/alternative urls if the env vars are present.
    /// Env vars can contain comma-separated URLs for fallback support.
    pub fn upgrade(mut self) -> Result<Self, VerifierError> {
        for (id, verifier) in self.verifiers.iter_mut() {
            // TODO: coda - update the chain id env var ids to preceded with "EIP155_"
            let eip_id = id.split(":").nth(1).ok_or(VerifierError::MalformedEipUrl)?;
            if let Ok(val) = std::env::var(format!("CHAIN_RPC_{eip_id}")) {
                let urls: Vec<Url> = val
                    .split(',')
                    .map(|s| s.trim().parse())
                    .collect::<Result<_, _>>()?;
                *verifier = make_verifier(urls)?;
            } else {
                info!("No upgraded chain url for chain {id}, using default.");
            };
        }

        if let Ok(url) = std::env::var("ANVIL_URL") {
            info!("Adding anvil from env to the verifiers: {url}");
            self.add_anvil(url)?;
        } else {
            use xmtp_configuration::DockerUrls;
            info!("adding default anvil url @{}", DockerUrls::ANVIL);
            self.add_anvil(DockerUrls::ANVIL.to_string())?;
        }
        Ok(self)
    }

    pub fn add_verifier(&mut self, id: String, url: String) -> Result<(), VerifierError> {
        self.verifiers
            .insert(id, Box::new(RpcSmartContractWalletVerifier::new(url)?));
        Ok(())
    }

    pub fn add_anvil(&mut self, url: String) -> Result<(), VerifierError> {
        self.verifiers.insert(
            "eip155:31337".to_string(),
            Box::new(RpcSmartContractWalletVerifier::new(url)?),
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
        if let Some(verifier) = self.verifiers.get(&account_id.chain_id) {
            return verifier
                .is_valid_signature(account_id, hash, signature, block_number)
                .await;
        }

        Err(VerifierError::NoVerifier(account_id.chain_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chain_urls_array_format() {
        let json = r#"{
            "eip155:1": ["https://rpc1.example.com", "https://rpc2.example.com"],
            "eip155:8453": ["https://rpc1.base.org"]
        }"#;
        let result = MultiSmartContractSignatureVerifier::parse_chain_urls(json).unwrap();
        assert_eq!(result["eip155:1"].len(), 2);
        assert_eq!(result["eip155:8453"].len(), 1);
    }

    #[test]
    fn test_parse_chain_urls_single_string_format() {
        let json = r#"{
            "eip155:1": "https://rpc1.example.com",
            "eip155:8453": "https://rpc1.base.org"
        }"#;
        let result = MultiSmartContractSignatureVerifier::parse_chain_urls(json).unwrap();
        assert_eq!(result["eip155:1"].len(), 1);
        assert_eq!(result["eip155:8453"].len(), 1);
    }

    #[test]
    fn test_parse_chain_urls_mixed_format() {
        let json = r#"{
            "eip155:1": ["https://rpc1.example.com", "https://rpc2.example.com"],
            "eip155:8453": "https://rpc1.base.org"
        }"#;
        let result = MultiSmartContractSignatureVerifier::parse_chain_urls(json).unwrap();
        assert_eq!(result["eip155:1"].len(), 2);
        assert_eq!(result["eip155:8453"].len(), 1);
    }

    #[test]
    fn test_parse_default_chain_urls() {
        let result =
            MultiSmartContractSignatureVerifier::parse_chain_urls(DEFAULT_CHAIN_URLS).unwrap();
        assert!(result.len() >= 11);
        assert!(!result["eip155:1"].is_empty());
    }

    #[test]
    fn test_make_verifier_single_url() {
        let urls = vec!["https://rpc1.example.com".parse().unwrap()];
        let verifier = make_verifier(urls);
        assert!(verifier.is_ok());
    }

    #[test]
    fn test_make_verifier_multiple_urls() {
        let urls = vec![
            "https://rpc1.example.com".parse().unwrap(),
            "https://rpc2.example.com".parse().unwrap(),
        ];
        let verifier = make_verifier(urls);
        assert!(verifier.is_ok());
    }

    #[test]
    fn test_make_verifier_empty_urls() {
        let urls: Vec<Url> = vec![];
        let verifier = make_verifier(urls);
        assert!(verifier.is_err());
    }

    #[test]
    fn test_new_with_multi_url_map() {
        let mut urls = HashMap::new();
        urls.insert(
            "eip155:1".to_string(),
            vec![
                "https://rpc1.example.com".parse().unwrap(),
                "https://rpc2.example.com".parse().unwrap(),
            ],
        );
        urls.insert(
            "eip155:8453".to_string(),
            vec!["https://rpc1.base.org".parse().unwrap()],
        );
        let verifier = MultiSmartContractSignatureVerifier::new(urls);
        assert!(verifier.is_ok());
    }
}
