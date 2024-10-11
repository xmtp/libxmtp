use wasm_timer::{SystemTime, UNIX_EPOCH};

pub const NS_IN_SEC: i64 = 1_000_000_000;

pub fn now_ns() -> i64 {
    let now = SystemTime::now();

    now.duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as i64
}

#[cfg(any(test, feature = "test-utils"))]
pub mod test {
    #![allow(clippy::unwrap_used)]
    use std::sync::Arc;

    use ethers::{
        contract::abigen,
        middleware::SignerMiddleware,
        providers::{Http, Provider},
        signers::{LocalWallet, Signer},
        utils::{Anvil, AnvilInstance},
    };
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

    /// Test harness that loads a local anvil node with deployed smart contracts.
    pub async fn with_smart_contracts<Func, Fut>(fun: Func)
    where
        Func: FnOnce(
            AnvilInstance,
            Provider<Http>,
            SignerMiddleware<Provider<Http>, LocalWallet>,
            SmartContracts,
        ) -> Fut,
        Fut: futures::Future<Output = ()>,
    {
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
}
