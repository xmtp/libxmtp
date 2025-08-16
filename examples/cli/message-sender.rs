use alloy::signers::local::PrivateKeySigner;
use clap::Parser;
use prost::Message;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info};
use xmtp_api_grpc::Client as GrpcApiClient;
use xmtp_api_grpc::GrpcError;
use xmtp_content_types::{text::TextCodec, ContentCodec};
use xmtp_db::{EncryptedMessageStore, NativeDb, StorageOption};
use xmtp_mls::context::XmtpMlsLocalContext;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::Client;
use xmtp_mls::InboxOwner;
use xmtp_proto::traits::ApiClientError;

#[derive(Parser)]
#[command(name = "message-sender")]
#[command(
    about = "XMTP Message Sender - Sends messages to a specific inbox ID using multiple threads"
)]
struct Args {
    /// Target inbox ID to send messages to
    #[arg(value_name = "inbox_id")]
    inbox_id: String,

    /// Number of threads to use for sending messages
    #[arg(long, default_value = "20")]
    threads: usize,

    /// Number of messages to send per thread
    #[arg(long, default_value = "500")]
    messages: usize,

    /// Output file for sent message IDs
    #[arg(long, default_value = "sent-message-ids.txt")]
    output_file: String,

    /// Save message IDs to output file
    #[arg(long, default_value = "false")]
    output: bool,
}

type MlsContext = Arc<
    XmtpMlsLocalContext<
        Arc<dyn xmtp_proto::api_client::BoxableXmtpApi<ApiClientError<GrpcError>>>,
        xmtp_db::DefaultStore,
        xmtp_db::DefaultMlsStore,
    >,
>;
type ClientType = Client<MlsContext>;

async fn create_client_with_wallet() -> Result<ClientType, Box<dyn std::error::Error + Send + Sync>>
{
    // Generate a random wallet for signing
    let wallet = PrivateKeySigner::random();
    info!("Created wallet address: {}", wallet.address());

    // Create XMTP client
    let nonce = 0;
    let ident = wallet.get_identifier().expect("Wallet address is invalid");
    let inbox_id = ident.inbox_id(nonce).expect("Failed to get inbox ID");

    // Create GRPC client for local node
    let api_client: Arc<dyn xmtp_proto::api_client::BoxableXmtpApi<ApiClientError<GrpcError>>> =
        Arc::new(
            GrpcApiClient::create("http://localhost:5556", false, None::<String>)
                .await
                .expect("Failed to create GRPC client"),
        );

    // Create encrypted store with ephemeral storage
    let native_db =
        NativeDb::new_unencrypted(&StorageOption::Ephemeral).expect("Failed to create native DB");
    let store = EncryptedMessageStore::new(native_db).expect("Failed to create store");

    let client = Client::builder(IdentityStrategy::new(inbox_id, ident, nonce, None))
        .store(store)
        .api_clients(api_client.clone(), api_client)
        .with_remote_verifier()
        .expect("Failed to configure remote verifier")
        .default_mls_store()
        .expect("Failed to configure MLS store")
        .build()
        .await
        .expect("Failed to build client");

    // Register the identity
    let mut signature_request = client.identity().signature_request().unwrap();
    let signature = wallet.sign(&signature_request.signature_text()).unwrap();
    signature_request
        .add_signature(signature, client.scw_verifier())
        .await
        .unwrap();

    if let Err(e) = client.register_identity(signature_request).await {
        error!("Identity registration failed: {}", e);
        return Err(e.into());
    }

    info!("Client created and registered successfully!");
    info!("Client Inbox ID: {}", client.inbox_id());

    Ok(client)
}

async fn send_messages_in_thread(
    thread_id: usize,
    target_inbox_id: String,
    messages_to_send: usize,
    message_ids: Arc<Mutex<Vec<String>>>,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Thread {} starting, will send {} messages",
        thread_id, messages_to_send
    );

    let client = create_client_with_wallet().await?;

    // Create or find DM with target inbox ID
    info!(
        "Thread {} creating DM with target inbox ID: {}",
        thread_id, target_inbox_id
    );

    let group = client
        .find_or_create_dm_by_inbox_id(target_inbox_id.clone(), None)
        .await
        .expect("Failed to create DM");

    info!(
        "Thread {} created/found DM: {}",
        thread_id,
        hex::encode(&group.group_id)
    );

    let mut sent_count = 0;

    for i in 0..messages_to_send {
        let message_text = format!("Message {} from thread {}", i + 1, thread_id);

        // Encode message content
        let encoded_content = TextCodec::encode(message_text.clone()).unwrap();
        let mut content_bytes = Vec::new();
        encoded_content.encode(&mut content_bytes).unwrap();

        // Send message
        match group.send_message(&content_bytes).await {
            Ok(message_id) => {
                sent_count += 1;
                let hex_id = hex::encode(&message_id);

                // Store message ID
                {
                    let mut ids = message_ids.lock().await;
                    ids.push(hex_id.clone());
                }

                info!(
                    "Thread {} sent message {}/{}: ID={}",
                    thread_id,
                    i + 1,
                    messages_to_send,
                    hex_id
                );
            }
            Err(e) => {
                error!(
                    "Thread {} failed to send message {}: {}",
                    thread_id,
                    i + 1,
                    e
                );
            }
        }
    }

    info!(
        "Thread {} completed, sent {} messages",
        thread_id, sent_count
    );
    Ok(sent_count)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging - only show logs from this CLI
    tracing_subscriber::fmt()
        .with_env_filter("message_sender=info")
        .init();

    let args = Args::parse();

    info!("Starting XMTP Message Sender");
    info!("Target inbox ID: {}", args.inbox_id);
    info!("Threads: {}", args.threads);
    info!("Messages per thread: {}", args.messages);
    info!("Total messages to send: {}", args.threads * args.messages);

    let message_ids = Arc::new(Mutex::new(Vec::new()));
    let start_time = Instant::now();

    // Spawn threads
    let mut handles = Vec::new();
    for thread_id in 0..args.threads {
        let target_inbox_id = args.inbox_id.clone();
        let message_ids_clone = Arc::clone(&message_ids);

        let handle = tokio::spawn(async move {
            send_messages_in_thread(thread_id, target_inbox_id, args.messages, message_ids_clone)
                .await
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    let mut total_sent = 0;
    for handle in handles {
        match handle.await {
            Ok(Ok(sent_count)) => {
                total_sent += sent_count;
            }
            Ok(Err(e)) => {
                error!("Thread failed: {}", e);
            }
            Err(e) => {
                error!("Thread panicked: {}", e);
            }
        }
    }

    let total_duration = start_time.elapsed();
    let duration_seconds = total_duration.as_secs_f64();
    let messages_per_second = if duration_seconds > 0.0 {
        total_sent as f64 / duration_seconds
    } else {
        0.0
    };

    info!("=== RESULTS ===");
    info!("Total messages sent: {}", total_sent);
    info!("Total duration: {:.2} seconds", duration_seconds);
    info!("Average messages per second: {:.2}", messages_per_second);

    // Write message IDs to file if enabled
    let message_ids_vec = message_ids.lock().await;
    if args.output && !message_ids_vec.is_empty() {
        info!(
            "Writing {} message IDs to {}",
            message_ids_vec.len(),
            args.output_file
        );
        let mut file = File::create(&args.output_file)?;
        for id in message_ids_vec.iter() {
            writeln!(file, "{}", id)?;
        }
        info!("Message IDs written to {}", args.output_file);
    } else if args.output {
        info!("No messages sent successfully, not creating output file");
    }

    info!("Message sender finished");
    Ok(())
}
