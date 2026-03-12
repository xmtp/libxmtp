//! Wallet funding utilities for xmtpd nodes
//!
//! Provides functionality to fund node wallets with testnet ETH using Anvil's
//! `anvil_setBalance` RPC method. This ensures nodes have sufficient balance
//! to pay for gas when interacting with smart contracts.

use alloy::{
    primitives::{Address, U256, utils::parse_ether},
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
    transports::http::reqwest::Url,
};
use color_eyre::eyre::{Context, Result};
use tracing::info;
use xmtp_common::{Retry, RetryableError, retry_async};

/// Default funding amount for new nodes (in ETH)
const DEFAULT_FUNDING_AMOUNT_ETH: &str = "1000";

/// Fund a wallet with testnet ETH using Anvil's `anvil_setBalance` RPC method.
///
/// This function directly sets the balance of the specified address using
/// Anvil's special RPC method, which is instant and doesn't require any
/// gas or transaction fees.
///
/// # Arguments
/// * `anvil_rpc_url` - The HTTP RPC URL for the Anvil instance
/// * `recipient_address` - The address to fund
/// * `amount_eth` - Optional amount in ETH to set (defaults to 1000 ETH)
///
/// # Errors
/// Returns an error if:
/// - The RPC connection fails
/// - The RPC call fails
/// - The amount cannot be parsed
pub async fn fund_wallet(
    anvil_rpc_url: &str,
    recipient_address: Address,
    amount_eth: Option<&str>,
) -> Result<()> {
    let amount = amount_eth.unwrap_or(DEFAULT_FUNDING_AMOUNT_ETH);

    info!(
        "Setting wallet {} balance to {} ETH using anvil_setBalance",
        recipient_address, amount
    );

    // Parse the amount to set
    let value: U256 = parse_ether(amount).wrap_err("Failed to parse ETH amount")?;

    // Build a basic provider without wallet
    let rpc_url: Url = anvil_rpc_url
        .parse()
        .wrap_err("Failed to parse Anvil RPC URL")?;
    let provider = ProviderBuilder::new().connect_http(rpc_url).erased();

    // Call anvil_setBalance RPC method
    retry_async!(
        Retry::default(),
        (async {
            provider
                .anvil_set_balance(recipient_address, value)
                .await
                .map_err(|e| SetBalanceFailure(Box::new(e)))
        })
    )?;

    info!(
        "Successfully set wallet {} balance to {} ETH",
        recipient_address, amount
    );

    Ok(())
}

pub struct SetBalanceFailure(Box<dyn std::error::Error + Send + Sync>);

impl std::error::Error for SetBalanceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl std::fmt::Display for SetBalanceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to set balance")
    }
}

impl std::fmt::Debug for SetBalanceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl RetryableError for SetBalanceFailure {
    fn is_retryable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test is disabled by default as it requires a running Anvil instance.
    /// Enable it manually for integration testing.
    #[ignore]
    #[tokio::test]
    async fn test_fund_wallet() {
        let anvil_rpc = "http://localhost:8545";
        // Use a test address (second Anvil account)
        let recipient: Address = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
            .parse()
            .unwrap();

        // This will use anvil_setBalance to instantly set the balance
        fund_wallet(anvil_rpc, recipient, Some("10")).await.unwrap();
    }
}
