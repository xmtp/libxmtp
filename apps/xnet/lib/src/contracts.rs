//! Direct smart contract interactions for broadcaster pause/unpause.
//!
//! Calls the AppChainParameterRegistry and broadcaster contracts directly
//! via alloy, avoiding the xmtpd-cli Docker container overhead.

use alloy::{
    primitives::{Address, FixedBytes},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
    sol,
    transports::http::reqwest::Url,
};
use color_eyre::eyre::{Context, Result, eyre};
use tracing::info;

use crate::constants::Anvil as AnvilConst;

sol! {
    #[sol(rpc)]
    interface IParameterRegistry {
        function set(string key, bytes32 value) external;
    }

    #[sol(rpc)]
    interface IBroadcaster {
        function paused() external view returns (bool);
        function updatePauseStatus() external;
    }
}

/// Broadcaster targets with their parameter keys and contract addresses.
const BROADCASTER_TARGETS: &[(&str, &str, &str)] = &[
    (
        "identity",
        "xmtp.identityUpdateBroadcaster.paused",
        AnvilConst::IDENTITY_BROADCASTER,
    ),
    (
        "group",
        "xmtp.groupMessageBroadcaster.paused",
        AnvilConst::GROUP_BROADCASTER,
    ),
    (
        "app-chain-gateway",
        "xmtp.appChainGateway.paused",
        AnvilConst::APP_CHAIN_GATEWAY,
    ),
];

/// Encode a bool as a `bytes32` value (matching Go's `packBool`).
fn encode_bool(value: bool) -> FixedBytes<32> {
    let mut bytes = [0u8; 32];
    if value {
        bytes[31] = 1;
    }
    FixedBytes::from(bytes)
}

/// Set the pause state of all broadcaster contracts.
///
/// This directly calls the AppChainParameterRegistry to set the pause flag,
/// then calls `updatePauseStatus()` on each broadcaster to sync the state.
/// Finally verifies each broadcaster reports the expected pause state.
pub async fn set_broadcasters_paused(
    rpc_url: &str,
    admin_key: &str,
    paused: bool,
) -> Result<()> {
    let action = if paused { "Pausing" } else { "Unpausing" };
    info!("{} broadcaster contracts", action);

    let rpc: Url = rpc_url.parse().wrap_err("Failed to parse RPC URL")?;
    let signer: PrivateKeySigner = admin_key.parse().wrap_err("Failed to parse admin key")?;
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(rpc);

    let registry_addr: Address = AnvilConst::PARAMETER_REGISTRY
        .parse()
        .wrap_err("Failed to parse parameter registry address")?;
    let registry = IParameterRegistry::new(registry_addr, &provider);

    let value = encode_bool(paused);

    // Set pause flag in the parameter registry for each broadcaster
    for &(name, key, _) in BROADCASTER_TARGETS {
        info!("Setting {} pause={} in registry", name, paused);
        registry
            .set(key.to_string(), value)
            .send()
            .await
            .wrap_err_with(|| format!("Failed to send set() for {}", name))?
            .get_receipt()
            .await
            .wrap_err_with(|| format!("Failed to confirm set() for {}", name))?;
    }

    // Call updatePauseStatus() on each broadcaster to sync from registry
    for &(name, _, addr_str) in BROADCASTER_TARGETS {
        let addr: Address = addr_str
            .parse()
            .wrap_err_with(|| format!("Failed to parse {} address", name))?;
        let broadcaster = IBroadcaster::new(addr, &provider);

        info!("Calling updatePauseStatus() on {}", name);
        broadcaster
            .updatePauseStatus()
            .send()
            .await
            .wrap_err_with(|| format!("Failed to send updatePauseStatus() for {}", name))?
            .get_receipt()
            .await
            .wrap_err_with(|| format!("Failed to confirm updatePauseStatus() for {}", name))?;
    }

    // Verify each broadcaster is in the expected state
    for &(name, _, addr_str) in BROADCASTER_TARGETS {
        let addr: Address = addr_str.parse()?;
        let broadcaster = IBroadcaster::new(addr, &provider);
        let actual = broadcaster.paused().call().await?;
        if actual != paused {
            return Err(eyre!(
                "{} broadcaster pause state mismatch: expected={}, actual={}",
                name,
                paused,
                actual
            ));
        }
        info!("{} broadcaster verified: paused={}", name, actual);
    }

    Ok(())
}

/// Query the pause status of all broadcaster contracts.
///
/// Returns a list of `(target_name, paused)` tuples.
pub async fn get_broadcaster_pause_status(
    rpc_url: &str,
) -> Result<Vec<(&'static str, bool)>> {
    let rpc: Url = rpc_url.parse().wrap_err("Failed to parse RPC URL")?;
    let provider = ProviderBuilder::new().connect_http(rpc);

    let mut statuses = Vec::new();
    for &(name, _, addr_str) in BROADCASTER_TARGETS {
        let addr: Address = addr_str
            .parse()
            .wrap_err_with(|| format!("Failed to parse {} address", name))?;
        let broadcaster = IBroadcaster::new(addr, &provider);
        let paused = broadcaster.paused().call().await?;
        statuses.push((name, paused));
    }

    Ok(statuses)
}
