#![allow(clippy::unwrap_used)]
#![cfg_attr(all(target_family = "wasm", target_os = "unknown"), allow(unused))]
use CoinbaseSmartWallet::CoinbaseSmartWalletInstance;
use CoinbaseSmartWalletFactory::CoinbaseSmartWalletFactoryInstance;
use alloy::network::{Ethereum, EthereumWallet};
use alloy::primitives::{Address, Bytes};
use alloy::providers::DynProvider;
use alloy::providers::Provider;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::sol_types::SolValue;
use alloy::{primitives::U256, providers::ext::AnvilApi};
use rstest::*;
use xmtp_configuration::DockerUrls;

sol!(
    #[derive(serde::Serialize, serde::Deserialize)]
    #[sol(rpc)]
    CoinbaseSmartWallet,
    "artifact/CoinbaseSmartWallet.json",
);

sol!(
    #[derive(serde::Serialize, serde::Deserialize)]
    #[sol(rpc)]
    CoinbaseSmartWalletFactory,
    "artifact/CoinbaseSmartWalletFactory.json",
);

#[cfg(not(target_arch = "wasm32"))]
pub async fn fund_user(user: PrivateKeySigner, anvil: impl AnvilApi<Ethereum>) {
    anvil
        .anvil_set_balance(
            user.address(),
            U256::from(1_000_000_000_000_000_000_000u128),
        )
        .await
        .unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
#[fixture]
pub async fn smart_wallet(#[future] spawned_provider: EthereumProvider) -> SmartWalletContext {
    deploy_wallets(spawned_provider.await).await
}

#[cfg(not(target_arch = "wasm32"))]
#[fixture]
pub async fn docker_smart_wallet(
    #[future] docker_provider: EthereumProvider,
) -> SmartWalletContext {
    deploy_wallets(docker_provider.await).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn deploy_wallets(provider: EthereumProvider) -> SmartWalletContext {
    use std::time::Duration;

    let EthereumProvider {
        provider,
        owner0,
        owner1,
        sc_owner,
        ..
    } = provider;
    let factory = cb_smart_wallet(sc_owner.clone(), provider.clone()).await;
    let nonce = U256::from(0); // needed when creating a smart wallet
    let owners_addresses = vec![
        Bytes::from(owner0.address().abi_encode()),
        Bytes::from(owner1.address().abi_encode()),
    ];
    let sw_address = factory
        .getAddress(owners_addresses.clone(), nonce)
        .call()
        .await
        .unwrap();
    println!("smart wallet address: {}", sw_address);
    let _ = factory
        .createAccount(owners_addresses.clone(), nonce)
        .send()
        .await
        .unwrap()
        .with_required_confirmations(1)
        .with_timeout(Some(Duration::from_secs(60)))
        .watch()
        .await
        .unwrap();
    let sw = CoinbaseSmartWallet::new(sw_address, provider.clone());

    SmartWalletContext {
        factory,
        sw,
        owner0,
        owner1,
        sw_address,
    }
}

pub struct EthereumProvider {
    pub provider: DynProvider,
    pub owner0: PrivateKeySigner,
    pub owner1: PrivateKeySigner,
    pub sc_owner: PrivateKeySigner,
    pub wallet: EthereumWallet,
}

#[cfg(not(target_arch = "wasm32"))]
async fn provider(url: Option<String>) -> EthereumProvider {
    let provider = ProviderBuilder::new();
    let sc_owner = PrivateKeySigner::random();
    let owner0 = PrivateKeySigner::random();
    let owner1 = PrivateKeySigner::random();
    let mut wallet = EthereumWallet::new(sc_owner.clone());
    wallet.register_signer(owner0.clone());
    wallet.register_signer(owner1.clone());
    println!("owner0: {}, owner1: {}", owner0.address(), owner1.address());
    let provider = provider.wallet(wallet.clone());
    let provider = if let Some(s) = url {
        provider.connect_http(s.parse().unwrap()).erased()
    } else {
        provider.connect_anvil().erased()
    };
    fund_user(owner0.clone(), &provider).await;
    fund_user(owner1.clone(), &provider).await;

    EthereumProvider {
        provider,
        owner0,
        owner1,
        sc_owner,
        wallet,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[fixture]
pub async fn spawned_provider() -> EthereumProvider {
    provider(None).await
}

#[cfg(not(target_arch = "wasm32"))]
#[fixture]
pub async fn docker_provider() -> EthereumProvider {
    provider(Some(DockerUrls::ANVIL.to_string())).await
}

pub struct SmartWalletContext {
    pub factory: CoinbaseSmartWalletFactoryInstance<DynProvider>,
    pub sw: CoinbaseSmartWalletInstance<DynProvider>,
    pub owner0: PrivateKeySigner,
    pub owner1: PrivateKeySigner,
    /// Address of the scw
    pub sw_address: Address,
}

pub type SignatureWithNonce = sol! { tuple(uint256, bytes) };

#[cfg(not(target_arch = "wasm32"))]
pub async fn cb_smart_wallet(
    sc_owner: PrivateKeySigner,
    provider: impl Provider + AnvilApi<Ethereum> + 'static,
) -> CoinbaseSmartWalletFactoryInstance<DynProvider> {
    provider
        .anvil_set_balance(
            sc_owner.address(),
            U256::from(1_000_000_000_000_000_000_000u128),
        )
        .await
        .unwrap();
    let provider = provider.erased();
    let smart_wallet = CoinbaseSmartWallet::deploy(provider.clone()).await.unwrap();
    CoinbaseSmartWalletFactory::deploy(provider, *smart_wallet.address())
        .await
        .unwrap()
}

// anvil can't be used in wasm because it is a system binary
/// Test harness that loads a local anvil node with deployed smart contracts.
#[cfg(not(target_arch = "wasm32"))]
pub async fn with_smart_contracts<Func, Fut>(fun: Func)
where
    Func: FnOnce(CoinbaseSmartWalletFactoryInstance<DynProvider>) -> Fut,
    Fut: futures::Future<Output = ()>,
{
    let sc_owner = PrivateKeySigner::random();
    let provider = ProviderBuilder::new()
        .wallet(sc_owner.clone())
        .connect_anvil_with_config(|anvil| anvil.args(vec!["--base-fee", "100"]));
    provider
        .anvil_set_balance(
            sc_owner.address(),
            U256::from(1_000_000_000_000_000_000_000u128),
        )
        .await
        .unwrap();
    let provider = provider.erased();
    let smart_wallet = CoinbaseSmartWallet::deploy(provider.clone()).await.unwrap();
    let factory = CoinbaseSmartWalletFactory::deploy(provider, *smart_wallet.address())
        .await
        .unwrap();

    fun(factory).await
}
