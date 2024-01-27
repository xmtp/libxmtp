use thiserror::Error;
use xmtp_cryptography::signature::is_valid_ethereum_address;

#[derive(Debug, Error)]
pub enum AddressValidationError {
    #[error("invalid addresses: {0:?}")]
    InvalidAddresses(Vec<String>),
}

pub fn sanitize_evm_addresses(
    account_addresses: Vec<String>,
) -> Result<Vec<String>, AddressValidationError> {
    let mut invalid = account_addresses
        .iter()
        .filter(|a| !is_valid_ethereum_address(a))
        .peekable();

    if invalid.peek().is_some() {
        return Err(AddressValidationError::InvalidAddresses(
            invalid.map(ToString::to_string).collect::<Vec<_>>(),
        ));
    }

    Ok(account_addresses
        .iter()
        .map(|address| address.to_lowercase())
        .collect())
}
