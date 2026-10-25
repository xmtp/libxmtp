//! The one checked form of an ERC-6492 verification input.
use super::VerifierError;
use crate::associations::AccountId;
use alloy::primitives::{BlockNumber, Bytes};
use xmtp_configuration::MAX_SCW_SIGNATURE_BYTES;

/// One ERC-6492 signature check whose input passed every check a validator
/// makes before chain access: an `eip155` account id with a chain id in
/// canonical decimal form and an address that is not zero, and a signature of
/// at most [`MAX_SCW_SIGNATURE_BYTES`].
///
/// The fields are private to this module, so [`ScwInput::checked`] is the only
/// constructor, and a verifier entry that takes one cannot reach a chain, a
/// cache or an encoder with any other input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScwInput {
    account_id: AccountId,
    chain_id: u64,
    hash: [u8; 32],
    signature: Bytes,
    block_number: Option<BlockNumber>,
}

impl ScwInput {
    /// Check one input, before any chain access. A malformed or zero account
    /// is [`VerifierError::InvalidAccountId`], and a signature longer than
    /// [`MAX_SCW_SIGNATURE_BYTES`] is [`VerifierError::SignatureTooLong`];
    /// neither is retryable.
    // implements: IDENT-093
    pub fn checked(
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<Self, VerifierError> {
        let chain_id = account_id
            .eip155_chain_id()
            .map_err(VerifierError::InvalidAccountId)?;
        if signature.len() > MAX_SCW_SIGNATURE_BYTES {
            return Err(VerifierError::SignatureTooLong(signature.len()));
        }
        Ok(Self {
            account_id,
            chain_id,
            hash,
            signature,
            block_number,
        })
    }

    /// The CAIP-10 account the signature is for.
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    /// The account's chain id.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The digest the signature signs.
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    /// The signature, as the wallet encoded it.
    pub fn signature(&self) -> &Bytes {
        &self.signature
    }

    /// The block to verify at; `None` is the chain's latest block.
    pub fn block_number(&self) -> Option<BlockNumber> {
        self.block_number
    }
}
