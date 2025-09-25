use alloy::signers::local::PrivateKeySigner;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use prost::Message;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};
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

    /// Timeout in seconds for each message send operation
    #[arg(long, default_value = "10")]
    timeout: u64,
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
        return Err(e.into());
    }

    Ok(client)
}

async fn send_messages_in_thread(
    thread_id: usize,
    target_inbox_id: String,
    messages_to_send: usize,
    message_ids: Arc<Mutex<Vec<String>>>,
    progress: Arc<ProgressBar>,
    timeout_seconds: u64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let client = create_client_with_wallet().await?;

    // Create or find DM with target inbox ID
    let group = client
        .find_or_create_dm_by_inbox_id(target_inbox_id.clone(), None)
        .await
        .expect("Failed to create DM");

    // Add 3 second delay after creating conversation and before sending messages
    info!(
        "Thread {}: Conversation created, waiting 3 seconds before sending messages...",
        thread_id
    );
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    info!("Thread {}: Starting to send messages", thread_id);

    let mut sent_count = 0;

    for i in 0..messages_to_send {
        let message_text = format!("Message {} from thread {}", i + 1, thread_id);

        // Encode message content
        let encoded_content = TextCodec::encode(message_text.clone()).unwrap();
        let mut content_bytes = Vec::new();
        encoded_content.encode(&mut content_bytes).unwrap();

        // Send message with timeout
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_seconds),
            group.send_message(&content_bytes),
        )
        .await
        {
            Ok(Ok(message_id)) => {
                sent_count += 1;
                let hex_id = hex::encode(&message_id);

                // Store message ID
                {
                    let mut ids = message_ids.lock().await;
                    ids.push(hex_id.clone());
                }

                // Update progress bar
                progress.inc(1);
            }
            Ok(Err(_e)) => {
                // Message send failed - continue without updating progress
            }
            Err(_) => {
                // Timeout occurred - continue without updating progress
            }
        }
    }

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
    info!("Threads: {} (max 20 concurrent)", args.threads);
    info!("Messages per thread: {}", args.messages);
    info!("Total messages to send: {}", args.threads * args.messages);
    info!("Message timeout: {} seconds", args.timeout);

    let message_ids = Arc::new(Mutex::new(Vec::new()));
    let start_time = Instant::now();

    // Create single progress bar for all threads
    let total_messages = args.threads * args.messages;
    let progress = Arc::new(ProgressBar::new(total_messages as u64));
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:50.cyan/blue}] {pos}/{len} ({percent}%) | {msg}")
            .expect("Failed to set progress bar template")
            .progress_chars("#>-"),
    );
    progress.set_message("Starting threads...");

    // Create semaphore to limit concurrent threads to 20
    const MAX_CONCURRENT_THREADS: usize = 20;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_THREADS));

    // Spawn a task to update progress bar message with current rate
    let rate_progress = Arc::clone(&progress);
    let active_threads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let active_threads_clone = Arc::clone(&active_threads);

    // Track min/max rates
    let min_rate = Arc::new(std::sync::Mutex::new(None::<f64>));
    let max_rate = Arc::new(std::sync::Mutex::new(None::<f64>));
    let min_rate_clone = Arc::clone(&min_rate);
    let max_rate_clone = Arc::clone(&max_rate);

    let rate_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(1000)); // 1 second intervals
        let mut last_position = 0u64;
        let mut last_update = Instant::now();

        loop {
            interval.tick().await;
            let now = Instant::now();
            let current_pos = rate_progress.position();
            let time_window = now.duration_since(last_update).as_secs_f64();

            if time_window > 0.0 {
                let messages_in_window = current_pos - last_position;
                let current_rate = messages_in_window as f64 / time_window;

                // Update min/max rates
                if current_rate > 0.0 {
                    if let Ok(mut min) = min_rate_clone.lock() {
                        *min = Some(min.map_or(current_rate, |m| m.min(current_rate)));
                    }
                    if let Ok(mut max) = max_rate_clone.lock() {
                        *max = Some(max.map_or(current_rate, |m| m.max(current_rate)));
                    }
                }

                let active = active_threads_clone.load(std::sync::atomic::Ordering::Relaxed);
                rate_progress.set_message(format!(
                    "{:.1} msg/s ({} active threads)",
                    current_rate, active
                ));

                // Reset for next window
                last_position = current_pos;
                last_update = now;
            }

            if rate_progress.is_finished() {
                break;
            }
        }
    });

    // Spawn threads with semaphore limiting
    let mut handles = Vec::new();
    for thread_id in 0..args.threads {
        let target_inbox_id = args.inbox_id.clone();
        let message_ids_clone = Arc::clone(&message_ids);
        let progress_clone = Arc::clone(&progress);
        let semaphore_clone = Arc::clone(&semaphore);
        let active_threads_clone = Arc::clone(&active_threads);

        let handle = tokio::spawn(async move {
            // Acquire permit before starting thread work
            let _permit = semaphore_clone.acquire().await.unwrap();

            // Increment active thread count
            active_threads_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let result = send_messages_in_thread(
                thread_id,
                target_inbox_id,
                args.messages,
                message_ids_clone,
                progress_clone,
                args.timeout,
            )
            .await;

            // Decrement active thread count
            active_threads_clone.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

            result
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

    // Finish progress bar
    progress.finish_with_message(format!(
        "Completed: {} messages sent at {:.1} msg/s",
        total_sent, messages_per_second
    ));

    // Wait for rate update task to finish
    let _ = rate_handle.await;

    info!("=== RESULTS ===");
    info!("Total messages sent: {}", total_sent);
    info!("Total duration: {:.2} seconds", duration_seconds);
    info!("Average messages per second: {:.2}", messages_per_second);

    // Log min/max rates if we have them
    if let (Ok(min), Ok(max)) = (min_rate.lock(), max_rate.lock()) {
        if let (Some(min_val), Some(max_val)) = (*min, *max) {
            info!(
                "Performance range: min {:.2} msg/s, max {:.2} msg/s",
                min_val, max_val
            );
        }
    }

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
