#![allow(clippy::unwrap_used)]

use ethers::{
    contract::abigen,
    core::k256::{elliptic_curve::SecretKey, Secp256k1},
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::LocalWallet,
};
use std::sync::LazyLock;

abigen!(
    CoinbaseSmartWallet,
    "artifact/CoinbaseSmartWallet.json",
    derives(serde::Serialize, serde::Deserialize)
);

abigen!(
    CoinbaseSmartWalletFactory,
    "artifact/CoinbaseSmartWalletFactory.json",
    derives(serde::Serialize, serde::Deserialize)
);

pub struct SmartContracts {
    coinbase_smart_wallet_factory:
        CoinbaseSmartWalletFactory<SignerMiddleware<Provider<Http>, LocalWallet>>,
}

impl SmartContracts {
    #[cfg(not(target_arch = "wasm32"))]
    fn new(
        coinbase_smart_wallet_factory: CoinbaseSmartWalletFactory<
            SignerMiddleware<Provider<Http>, LocalWallet>,
        >,
    ) -> Self {
        Self {
            coinbase_smart_wallet_factory,
        }
    }

    pub fn coinbase_smart_wallet_factory(
        &self,
    ) -> &CoinbaseSmartWalletFactory<SignerMiddleware<Provider<Http>, LocalWallet>> {
        &self.coinbase_smart_wallet_factory
    }
}

pub static ANVIL_KEYS: LazyLock<Vec<SecretKey<Secp256k1>>> = LazyLock::new(|| {
    vec![
        SecretKey::from_slice(
            hex::decode("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("dbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
        SecretKey::from_slice(
            hex::decode("2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6")
                .unwrap()
                .as_slice(),
        )
        .unwrap(),
    ]
});

pub struct AnvilMeta {
    pub keys: Vec<SecretKey<Secp256k1>>,
    pub endpoint: String,
    pub chain_id: u64,
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn with_docker_smart_contracts<Func, Fut>(fun: Func)
where
    Func: FnOnce(
        AnvilMeta,
        Provider<Http>,
        SignerMiddleware<Provider<Http>, LocalWallet>,
        SmartContracts,
    ) -> Fut,
    Fut: futures::Future<Output = ()>,
{
    use ethers::signers::Signer;
    use std::sync::Arc;

    let keys = ANVIL_KEYS.clone();
    let anvil_meta = AnvilMeta {
        keys: keys.clone(),
        chain_id: 31337,
        endpoint: "http://localhost:8545".to_string(),
    };

    let contract_deployer: LocalWallet = keys[9].clone().into();
    let provider = Provider::<Http>::try_from(&anvil_meta.endpoint).unwrap();
    let client = SignerMiddleware::new(
        provider.clone(),
        contract_deployer.clone().with_chain_id(anvil_meta.chain_id),
    );
    // 1. coinbase smart wallet
    // deploy implementation for factory
    let implementation = CoinbaseSmartWallet::deploy(Arc::new(client.clone()), ())
        .unwrap()
        .gas_price(100)
        .send()
        .await
        .unwrap();
    // deploy factory
    let factory =
        CoinbaseSmartWalletFactory::deploy(Arc::new(client.clone()), implementation.address())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();

    let smart_contracts = SmartContracts::new(factory);
    fun(
        anvil_meta,
        provider.clone(),
        client.clone(),
        smart_contracts,
    )
    .await
}

// anvil can't be used in wasm because it is a system binary
/// Test harness that loads a local anvil node with deployed smart contracts.
#[cfg(not(target_arch = "wasm32"))]
pub async fn with_smart_contracts<Func, Fut>(fun: Func)
where
    Func: FnOnce(
        ethers::utils::AnvilInstance,
        Provider<Http>,
        SignerMiddleware<Provider<Http>, LocalWallet>,
        SmartContracts,
    ) -> Fut,
    Fut: futures::Future<Output = ()>,
{
    use ethers::signers::Signer;
    use ethers::utils::Anvil;
    use std::sync::Arc;
    let anvil = Anvil::new().args(vec!["--base-fee", "100"]).spawn();
    let contract_deployer: LocalWallet = anvil.keys()[9].clone().into();
    let provider = Provider::<Http>::try_from(anvil.endpoint()).unwrap();
    let client = SignerMiddleware::new(
        provider.clone(),
        contract_deployer.clone().with_chain_id(anvil.chain_id()),
    );
    // 1. coinbase smart wallet
    // deploy implementation for factory
    let implementation = CoinbaseSmartWallet::deploy(Arc::new(client.clone()), ())
        .unwrap()
        .gas_price(100)
        .send()
        .await
        .unwrap();
    // deploy factory
    let factory =
        CoinbaseSmartWalletFactory::deploy(Arc::new(client.clone()), implementation.address())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();

    let smart_contracts = SmartContracts::new(factory);
    fun(anvil, provider.clone(), client.clone(), smart_contracts).await
}
