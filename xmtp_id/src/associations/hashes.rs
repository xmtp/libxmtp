use std::str::FromStr;

use ethers::{types::H160, utils::to_checksum};
use sha2::{Digest, Sha256};

use super::AssociationError;

/// Helper function to generate a SHA256 hash as a hex string.
fn sha256_string(input: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Validates that the account address is exactly 42 characters, starts with "0x",
/// and contains only valid hex digits.
fn is_valid_address(account_address: &str) -> bool {
    account_address.len() == 42
        && account_address.starts_with("0x")
        && account_address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Generates an inbox ID if the account address is valid.
pub fn generate_inbox_id(account_address: &str, nonce: &u64) -> Result<String, AssociationError> {
    if !is_valid_address(account_address) {
        return Err(AssociationError::InvalidAccountAddress);
    }
    Ok(sha256_string(format!(
        "{}{}",
        account_address.to_lowercase(),
        nonce
    )))
}

pub fn generate_inbox_id_from_checksum(
    account_address: &str,
    nonce: &u64,
) -> Result<String, AssociationError> {
    let checksum_address = to_checksum(
        &H160::from_str(account_address).map_err(|_| AssociationError::InvalidAccountAddress)?,
        None,
    );
    generate_inbox_id(&checksum_address, nonce)
}
