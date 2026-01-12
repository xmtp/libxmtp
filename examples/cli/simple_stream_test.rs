use alloy::signers::local::PrivateKeySigner;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_configuration::GrpcUrlsProduction;
use xmtp_cryptography::signature::{IdentifierValidationError, SignatureError};
use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb, StorageOption};
use xmtp_id::associations::{unverified::UnverifiedSignature, Identifier};
use xmtp_mls::context::{XmtpMlsLocalContext, XmtpSharedContext};
use xmtp_mls::{identity::IdentityStrategy, InboxOwner, XmtpApiClient};
use xmtp_proto::types::ApiIdentifier;

type MlsContext = Arc<XmtpMlsLocalContext<XmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;
type Client = xmtp_mls::client::Client<MlsContext>;

enum Wallet { Local(PrivateKeySigner) }

impl InboxOwner for Wallet {
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError> {
        let Wallet::Local(w) = self; w.get_identifier()
    }
    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let Wallet::Local(w) = self; w.sign(text)
    }
}

async fn create_client(name: &str) -> color_eyre::eyre::Result<(Client, Wallet)> {
    let grpc: XmtpApiClient = MessageBackendBuilder::default().v3_host(GrpcUrlsProduction::NODE).is_secure(true).build()?;
    let wallet = Wallet::Local(PrivateKeySigner::random());
    let ident = wallet.get_identifier()?;
    let inbox_id = ident.inbox_id(0)?;
    let store = EncryptedMessageStore::new(NativeDb::new(
        &StorageOption::Persistent(format!("/tmp/stream_test_{name}_{}.db", chrono::Utc::now().timestamp_millis())),
        [2u8; 32] as EncryptionKey,
    )?)?;
    let client: Client = xmtp_mls::Client::builder(IdentityStrategy::new(inbox_id, ident, 0, None))
        .store(store).api_clients(grpc.clone(), grpc).with_remote_verifier()?.default_mls_store()?.build().await?;
    let mut sig_req = client.identity().signature_request().unwrap();
    sig_req.add_signature(wallet.sign(&sig_req.signature_text())?, client.scw_verifier()).await?;
    client.register_identity(sig_req).await?;
    Ok((client, wallet))
}

#[tokio::main]
async fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_env_filter("xmtp_mls::subscriptions=debug").init();

    let (client_a, _) = create_client("A").await?;
    let (client_b, _) = create_client("B").await?;
    let inbox_b = client_b.context.inbox_id().to_string();

    let benny_inbox: Option<String> = {
        let id = Identifier::eth("0x9275fd4dc9a482673fbb0d145a490a71cd5125c0")?;
        let api_id: ApiIdentifier = id.into();
        client_a.context.api().get_inbox_ids(vec![api_id.clone()]).await?.get(&api_id).cloned()
    };

    let client_b2 = client_b.clone();
    let stream_task = tokio::spawn(async move {
        let mut stream = client_b2.stream_conversations(None, false).await.unwrap();
        while let Some(Ok(g)) = stream.next().await {
            println!("[STREAM] {}", hex::encode(&g.group_id));
        }
    });
    tokio::time::sleep(Duration::from_secs(2)).await;

    let g1 = client_a.create_group(None, None)?;
    g1.add_members_by_inbox_id(&[inbox_b.clone()]).await?;
    println!("[A] group1: {}", hex::encode(&g1.group_id));
    tokio::time::sleep(Duration::from_secs(5)).await;

    if let Some(ref benny) = benny_inbox {
        let g2 = client_a.create_group(None, None)?;
        g2.add_members_by_inbox_id(&[inbox_b.clone(), benny.clone()]).await?;
        println!("[A] group2: {}", hex::encode(&g2.group_id));
        tokio::time::sleep(Duration::from_secs(5)).await;

        let synced = client_b.sync_welcomes().await?;
        println!("[B] sync found {} groups", synced.len());
    }

    stream_task.abort();
    Ok(())
}
