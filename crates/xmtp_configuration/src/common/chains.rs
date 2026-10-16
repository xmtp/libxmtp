//! Chain identifier rules shared by the backend and the client.

/// Why a chain identifier is not a routable `eip155` chain id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Eip155ChainIdError {
    #[error("chain id is not prefixed with eip155:")]
    MissingPrefix,
    #[error("chain id is not a u64 in canonical decimal form")]
    NotCanonical,
}

/// The chain id of an `eip155` CAIP-2 identifier, whose reference must be the
/// chain id in canonical decimal form: no sign, no leading zero.
///
/// The backend accepts a `chains` key only when this parses it, and a client
/// routes a smart contract wallet signature only when it parses the account's
/// chain, so both sides agree on which chains exist.
pub fn parse_eip155_chain_id(chain: &str) -> Result<u64, Eip155ChainIdError> {
    let reference = chain
        .strip_prefix("eip155:")
        .ok_or(Eip155ChainIdError::MissingPrefix)?;
    reference
        .parse::<u64>()
        .ok()
        .filter(|chain_id| chain_id.to_string() == reference)
        .ok_or(Eip155ChainIdError::NotCanonical)
}

#[cfg(test)]
mod tests;
