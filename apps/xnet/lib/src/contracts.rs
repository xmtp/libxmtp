//! Direct smart contract interactions for broadcaster pause/unpause.
//!
//! Calls the AppChainParameterRegistry and broadcaster contracts directly
//! via alloy, avoiding the xmtpd-cli Docker container overhead.

use alloy::{
    primitives::{Address, B256},
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
        function updatePayloadBootstrapper() external;
        function payloadBootstrapper() external view returns (address);
    }
}

/// The `NoChange()` error selector (0xa88ee577).
/// Contracts revert with this when updatePauseStatus() or updatePayloadBootstrapper()
/// is called but the state hasn't actually changed.
const NO_CHANGE_SELECTOR: &str = "a88ee577";

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

/// Bootstrapper targets: (name, registry_key, broadcaster_address).
const BOOTSTRAPPER_TARGETS: &[(&str, &str, &str)] = &[
    (
        "identity",
        "xmtp.identityUpdateBroadcaster.payloadBootstrapper",
        AnvilConst::IDENTITY_BROADCASTER,
    ),
    (
        "group",
        "xmtp.groupMessageBroadcaster.payloadBootstrapper",
        AnvilConst::GROUP_BROADCASTER,
    ),
];

/// Set the pause state of all broadcaster contracts.
///
/// This directly calls the AppChainParameterRegistry to set the pause flag,
/// then calls `updatePauseStatus()` on each broadcaster to sync the state.
/// Finally verifies each broadcaster reports the expected pause state.
pub async fn set_broadcasters_paused(rpc_url: &str, admin_key: &str, paused: bool) -> Result<()> {
    let action = if paused { "Pausing" } else { "Unpausing" };
    info!("{} broadcaster contracts", action);

    let rpc: Url = rpc_url.parse().wrap_err("Failed to parse RPC URL")?;
    let signer: PrivateKeySigner = admin_key.parse().wrap_err("Failed to parse admin key")?;
    let provider = ProviderBuilder::new().wallet(signer).connect_http(rpc);

    let registry_addr: Address = AnvilConst::PARAMETER_REGISTRY
        .parse()
        .wrap_err("Failed to parse parameter registry address")?;
    let registry = IParameterRegistry::new(registry_addr, &provider);

    let value = if paused {
        B256::left_padding_from(&[1u8])
    } else {
        B256::ZERO
    };

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

    // Call updatePauseStatus() on each broadcaster to sync from registry.
    // NoChange() reverts are expected if the state is already correct.
    for &(name, _, addr_str) in BROADCASTER_TARGETS {
        let addr: Address = addr_str
            .parse()
            .wrap_err_with(|| format!("Failed to parse {} address", name))?;
        let broadcaster = IBroadcaster::new(addr, &provider);

        info!("Calling updatePauseStatus() on {}", name);
        match broadcaster.updatePauseStatus().send().await {
            Ok(pending) => {
                pending.get_receipt().await.wrap_err_with(|| {
                    format!("Failed to confirm updatePauseStatus() for {}", name)
                })?;
            }
            Err(e) if is_no_change_error(&e) => {
                info!("{} pause status already up to date (NoChange)", name);
            }
            Err(e) => {
                return Err(e)
                    .wrap_err_with(|| format!("Failed to send updatePauseStatus() for {}", name));
            }
        }
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

/// Set the payload bootstrapper address on the identity and group broadcaster contracts.
///
/// The payload bootstrapper is the address authorized to call `bootstrapIdentityUpdates()`
/// and `bootstrapGroupMessages()` during migration. This must be the migration payer's address.
pub async fn set_payload_bootstrapper(
    rpc_url: &str,
    admin_key: &str,
    bootstrapper: Address,
) -> Result<()> {
    info!(
        "Setting payload bootstrapper to {} on broadcaster contracts",
        bootstrapper
    );

    let rpc: Url = rpc_url.parse().wrap_err("Failed to parse RPC URL")?;
    let signer: PrivateKeySigner = admin_key.parse().wrap_err("Failed to parse admin key")?;
    let provider = ProviderBuilder::new().wallet(signer).connect_http(rpc);

    let registry_addr: Address = AnvilConst::PARAMETER_REGISTRY
        .parse()
        .wrap_err("Failed to parse parameter registry address")?;
    let registry = IParameterRegistry::new(registry_addr, &provider);

    let value = bootstrapper.into_word();

    for &(name, key, _) in BOOTSTRAPPER_TARGETS {
        info!("Setting {} payloadBootstrapper in registry", name);
        registry
            .set(key.to_string(), value)
            .send()
            .await
            .wrap_err_with(|| format!("Failed to send set() for {} bootstrapper", name))?
            .get_receipt()
            .await
            .wrap_err_with(|| format!("Failed to confirm set() for {} bootstrapper", name))?;
    }

    // Call updatePayloadBootstrapper() on each broadcaster to sync from registry.
    // NoChange() reverts are expected if the bootstrapper is already set correctly.
    for &(name, _, addr_str) in BOOTSTRAPPER_TARGETS {
        let addr: Address = addr_str
            .parse()
            .wrap_err_with(|| format!("Failed to parse {} address", name))?;
        let broadcaster = IBroadcaster::new(addr, &provider);

        info!("Calling updatePayloadBootstrapper() on {}", name);
        match broadcaster.updatePayloadBootstrapper().send().await {
            Ok(pending) => {
                pending.get_receipt().await.wrap_err_with(|| {
                    format!("Failed to confirm updatePayloadBootstrapper() for {}", name)
                })?;
            }
            Err(e) if is_no_change_error(&e) => {
                info!(
                    "{} payload bootstrapper already up to date (NoChange)",
                    name
                );
            }
            Err(e) => {
                return Err(e).wrap_err_with(|| {
                    format!("Failed to send updatePayloadBootstrapper() for {}", name)
                });
            }
        }
    }

    // Verify
    for &(name, _, addr_str) in BOOTSTRAPPER_TARGETS {
        let addr: Address = addr_str.parse()?;
        let broadcaster = IBroadcaster::new(addr, &provider);
        let actual = broadcaster.payloadBootstrapper().call().await?;
        if actual != bootstrapper {
            return Err(eyre!(
                "{} bootstrapper mismatch: expected={}, actual={}",
                name,
                bootstrapper,
                actual
            ));
        }
        info!("{} bootstrapper verified: {}", name, actual);
    }

    Ok(())
}

/// Query the pause status of all broadcaster contracts.
///
/// Returns a list of `(target_name, paused)` tuples.
pub async fn get_broadcaster_pause_status(rpc_url: &str) -> Result<Vec<(&'static str, bool)>> {
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

/// Check if an error is the `NoChange()` revert (selector 0xa88ee577).
fn is_no_change_error(e: &impl std::fmt::Display) -> bool {
    let msg = e.to_string();
    msg.contains(NO_CHANGE_SELECTOR)
}
