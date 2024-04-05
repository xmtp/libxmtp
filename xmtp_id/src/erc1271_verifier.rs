use anyhow::Error;
use ethers::contract::abigen;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address, BlockNumber, Bytes};
use std::convert::TryFrom;
use std::sync::Arc;

const EIP1271_MAGIC_VALUE: [u8; 4] = [0x16, 0x26, 0xba, 0x7e];

abigen!(
    ERC1271,
    r#"[
        function isValidSignature(bytes32 hash, bytes calldata signature) public view virtual returns (bytes4 result)
    ]"#,
    derives(serde::Serialize, serde::Deserialize)
);

pub struct ERC1271Verifier {
    pub provider: Arc<Provider<Http>>,
}

impl ERC1271Verifier {
    pub fn new(url: &str) -> Self {
        let provider = Arc::new(Provider::<Http>::try_from(url).unwrap());
        Self { provider }
    }

    /// Verifies an ERC-1271(https://eips.ethereum.org/EIPS/eip-1271) signature.
    ///
    /// # Arguments
    ///
    /// * `wallet_address` - Address of the ERC1271 wallet.
    /// * `block_number` - Block number to verify the signature at.
    /// * `hash`, `signature` - Inputs to ERC-1271, used for signer verification.
    pub async fn is_valid_signature<M: Middleware>(
        &self,
        wallet_address: Address,
        block_number: BlockNumber,
        hash: [u8; 32],
        signature: Bytes,
    ) -> Result<bool, Error> {
        let erc1271 = ERC1271::new(wallet_address, self.provider.clone());

        let res: [u8; 4] = erc1271
            .is_valid_signature(hash, signature)
            .block(block_number)
            .call()
            .await?
            .into();

        Ok(res == EIP1271_MAGIC_VALUE)
    }
}
