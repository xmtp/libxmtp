use ethers::types::Bytes;
use crate::associations::AccountId;
use crate::scw_verifier::BlockNumber;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct CacheKey {
    pub chain_id: String,
    pub account: String,
    pub hash: [u8; 32],
    pub signature: Vec<u8>,
    pub block_number: Option<u64>,
}

impl CacheKey {
    pub fn new(
        account_id: &AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        block_number: Option<BlockNumber>,
    ) -> Self {
        let block_number_u64 = block_number
            .and_then(|bn| bn.as_number().map(|n| n.as_u64()));
        Self {
            chain_id: account_id.chain_id.clone(),
            account: account_id.account_address.to_string(),
            hash,
            signature: signature.to_vec(),
            block_number: block_number_u64,
        }
    }
}
