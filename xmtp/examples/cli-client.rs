extern crate ethers;
extern crate log;
extern crate xmtp;

use clap::{arg, Parser};
use ethers_core::types::H160;
use log::{error, info};
use thiserror::Error;
use url::ParseError;
use walletconnect::client::{CallError, ConnectorError, SessionError};
use walletconnect::{qr, Client as WcClient, Metadata};
use xmtp::builder::AccountStrategy;
use xmtp::networking::MockXmtpApiClient;
use xmtp::persistence::in_memory_persistence::InMemoryPersistence;
use xmtp::storage::{EncryptedMessageStore, EncryptedMessageStoreError, StorageOption};
use xmtp::InboxOwner;
use xmtp_cryptography::signature::{h160addr_to_string, RecoverableSignature, SignatureError};
use xmtp_cryptography::utils::{rng, LocalWallet};

/// These are the command line arguments
#[derive(Parser)]
struct Args {
    /// Register using WalletConnect
    #[arg(short, long)]
    walletconnect: bool,
    /// Register using an Ethers LocalWallet
    #[arg(short, long)]
    localwallet: bool,
}

#[derive(Debug, Error)]
enum CliError {
    #[error("Walletconnect connection failed")]
    WcConnection(#[from] ConnectorError),
    #[error("Walletconnect session failed")]
    WcSession(#[from] SessionError),
    #[error("Walletconnect parse failed")]
    WcParse(#[from] ParseError),
    #[error("Walletconnect call failed")]
    WcCall(#[from] CallError),
    #[error("signature failed to generate")]
    Signature(#[from] SignatureError),
    #[error("stored error occured")]
    MessageStore(#[from] EncryptedMessageStoreError),
}

/// This is an abstraction which allows the CLI to choose between different wallet types.
enum Wallet {
    WalletConnectWallet(WalletConnectWallet),
    LocalWallet(LocalWallet),
}

impl InboxOwner for Wallet {
    fn get_address(&self) -> String {
        match self {
            Wallet::WalletConnectWallet(w) => w.get_address(),
            Wallet::LocalWallet(w) => w.get_address(),
        }
    }

    fn sign(
        &self,
        text: xmtp::association::AssociationText,
    ) -> Result<RecoverableSignature, SignatureError> {
        match self {
            Wallet::WalletConnectWallet(w) => w.sign(text),
            Wallet::LocalWallet(w) => w.sign(text),
        }
    }
}

/// A complete example of a minimal xmtp client which can send and recieve messages.
/// run this example from the cli:  `RUST_LOG=DEBUG cargo run --example cli-client`
#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting CLI Client....");

    let args = Args::parse();

    let msg_store = get_encrypted_store().unwrap();
    let wallet = AccountStrategy::CreateIfNotFound(get_wallet(&args).await.unwrap());

    let client_result = xmtp::ClientBuilder::new(wallet)
        .network(xmtp::Network::Dev)
        .api_client(MockXmtpApiClient::default())
        .persistence(InMemoryPersistence::default())
        .store(msg_store)
        .build();

    let mut client = match client_result {
        Err(e) => {
            error!("ClientBuilder Error: {:?}", e);
            return;
        }
        Ok(c) => c,
    };

    if let Err(e) = client.init().await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };

    // Application logic
    // ...

    info!("Exiting CLI Client....");
}

async fn get_wallet(args: &Args) -> Result<Wallet, CliError> {
    if args.walletconnect {
        return Ok(Wallet::WalletConnectWallet(
            WalletConnectWallet::create().await?,
        ));
    }
    info!("Fallback to LocalWallet");
    Ok(Wallet::LocalWallet(LocalWallet::new(&mut rng())))
}

fn get_encrypted_store() -> Result<EncryptedMessageStore, CliError> {
    EncryptedMessageStore::new(
        StorageOption::Ephemeral,
        EncryptedMessageStore::generate_enc_key(),
    )
    .map_err(|e| e.into())
}

/// This wraps a Walletconnect::client into a struct which could be used in the xmtp::client.
struct WalletConnectWallet {
    addr: String,
    client: WcClient,
}

impl WalletConnectWallet {
    pub async fn create() -> Result<Self, CliError> {
        let client = WcClient::new(
            "examples-cli",
            Metadata {
                description: "XMTP CLI.".into(),
                url: "https://github.com/xmtp/libxmtp".parse()?,
                icons: vec![
                    "https://gateway.ipfs.io/ipfs/QmaSZuaXfNUwhF7khaRxCwbhohBhRosVX1ZcGzmtcWnqav"
                        .parse()?,
                ],
                name: "XMTP CLI".into(),
            },
        )?;

        let (accounts, _) = client.ensure_session(qr::print_with_url).await?;

        for account in &accounts {
            info!(" Connected account: {:?}", account);
        }

        Ok(Self {
            addr: h160addr_to_string(H160::from_slice(accounts[0].as_bytes())),
            client,
        })
    }
}

impl InboxOwner for WalletConnectWallet {
    fn get_address(&self) -> String {
        self.addr.clone()
    }

    fn sign(
        &self,
        text: xmtp::association::AssociationText,
    ) -> Result<
        xmtp_cryptography::signature::RecoverableSignature,
        xmtp_cryptography::signature::SignatureError,
    > {
        let sig = futures::executor::block_on(async {
            self.client.personal_sign(&[text.text().as_str()]).await
        })
        .map_err(|e| SignatureError::ThirdPartyError(e.to_string()))?;

        Ok(RecoverableSignature::Eip191Signature(sig.to_vec()))
    }
}
