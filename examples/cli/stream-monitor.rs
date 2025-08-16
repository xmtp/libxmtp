use alloy::signers::local::PrivateKeySigner;
use clap::Parser;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{Duration, timeout};
use tracing::{error, info};
use xmtp_api_grpc::Client as GrpcApiClient;
use xmtp_db::{EncryptedMessageStore, NativeDb, StorageOption};
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::Client;
use xmtp_mls::InboxOwner;

#[derive(Parser)]
#[command(name = "stream-monitor")]
#[command(about = "XMTP Stream Monitor - Listens for all messages using stream_all_messages")]
struct Args {
    /// Optional timeout in seconds for stream monitoring
    #[arg(long, default_value = "10")]
    timeout: u64,

    /// Output file for message IDs
    #[arg(long, default_value = "received-message-ids.txt")]
    output_file: String,

    /// Maximum number of messages to receive before ending
    #[arg(long, default_value = "10000")]
    max_messages: usize,

    /// Use persistent database storage instead of ephemeral storage
    #[arg(long, default_value = "false")]
    use_database: bool,

    /// Save message IDs to output file
    #[arg(long, default_value = "false")]
    output: bool,
}

fn client_random_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}", timestamp)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging - only show logs from this CLI
    tracing_subscriber::fmt()
        .with_env_filter("stream_monitor=trace")
        .init();

    let args = Args::parse();

    info!("Starting XMTP Stream Monitor");

    // Generate a random wallet for signing
    let wallet = PrivateKeySigner::random();
    info!("Generated wallet address: {}", wallet.address());

    // Create XMTP client
    info!("Creating XMTP client...");
    let nonce = 0;
    let ident = wallet.get_identifier().expect("Wallet address is invalid");
    let inbox_id = ident.inbox_id(nonce).expect("Failed to get inbox ID");

    // Create GRPC client for local node
    let api_client = Arc::new(
        GrpcApiClient::create("http://localhost:5556", false, None::<String>)
            .await
            .expect("Failed to create GRPC client"),
    );

    // Create encrypted store based on CLI option
    let store = if args.use_database {
        let db_path = format!("stream-monitor-{}.db3", client_random_suffix());
        info!("Using persistent database: {}", db_path);
        let native_db = NativeDb::new_unencrypted(&StorageOption::Persistent(db_path))
            .expect("Failed to create native DB");
        EncryptedMessageStore::new(native_db).expect("Failed to create store")
    } else {
        info!("Using ephemeral storage");
        let native_db = NativeDb::new_unencrypted(&StorageOption::Ephemeral)
            .expect("Failed to create native DB");
        EncryptedMessageStore::new(native_db).expect("Failed to create store")
    };

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
    info!("Inbox ID: {}", client.inbox_id());

    // Start streaming all messages
    info!("Starting to stream all messages...");
    let mut message_stream = client.stream_all_messages(None, None).await?;

    let mut message_ids = Vec::with_capacity(args.max_messages); // Pre-allocate capacity
    let mut message_count = 0;

    // Timeout will start when first message is received and reset on each new message
    let timeout_duration = Duration::from_secs(args.timeout);
    let mut first_message_time: Option<std::time::Instant> = None;
    let mut last_message_time: Option<std::time::Instant> = None;

    // Create progress bar
    let progress = ProgressBar::new(args.max_messages as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | {msg}")
            .expect("Failed to set progress bar template")
            .progress_chars("#>-")
    );
    progress.set_message(format!("Waiting for messages... (timeout: {}s after last message)", args.timeout));

    loop {
        // Check for message limit first (fastest check)
        if message_count >= args.max_messages {
            info!("Message limit reached: {} messages", args.max_messages);
            break;
        }

        // Handle timeout logic based on whether we've received messages
        if let Some(last_msg_time) = last_message_time {
            // We've received at least one message, apply timeout logic
            let elapsed = last_msg_time.elapsed();
            if elapsed >= timeout_duration {
                info!(
                    "Timeout reached: {} seconds since last message",
                    args.timeout
                );
                break;
            }
            
            // Calculate remaining timeout and poll with timeout
            let remaining_timeout = timeout_duration - elapsed;
            match timeout(remaining_timeout, message_stream.next()).await {
                Ok(Some(Ok(message))) => {
                    message_count += 1;
                    let now = Instant::now();

                    // Reset timeout timer on each message
                    last_message_time = Some(now);

                    // Store message ID
                    message_ids.push(hex::encode(&message.id));

                    // Update progress bar with rate calculation
                    progress.inc(1);
                    if let Some(first_time) = first_message_time {
                        let elapsed = now.duration_since(first_time).as_secs_f64();
                        if elapsed > 0.0 {
                            let rate = message_count as f64 / elapsed;
                            progress.set_message(format!("{:.1} msg/s", rate));
                        }
                    } else {
                        progress.set_message("First message received!");
                    }
                }
                Ok(Some(Err(e))) => {
                    error!("Error receiving message: {}", e);
                    continue;
                }
                Ok(None) => {
                    info!("Stream ended");
                    break;
                }
                Err(_) => {
                    // Timeout occurred after receiving messages
                    info!(
                        "Timeout reached: {} seconds since last message",
                        args.timeout
                    );
                    break;
                }
            }
        } else {
            // No messages received yet, wait indefinitely for first message
            match message_stream.next().await {
                Some(Ok(message)) => {
                    message_count += 1;
                    let now = Instant::now();

                    // Start timer on first message
                    first_message_time = Some(now);
                    last_message_time = Some(now);

                    // Store message ID
                    message_ids.push(hex::encode(&message.id));

                    // Update progress bar for first message
                    progress.inc(1);
                    progress.set_message("First message received! Timer started.");
                }
                Some(Err(e)) => {
                    error!("Error receiving message: {}", e);
                    continue;
                }
                None => {
                    info!("Stream ended");
                    break;
                }
            }
        }
    }

    // Finish progress bar
    progress.finish_with_message(format!("Completed: {} messages received", message_count));

    info!(
        "Stream monitoring completed. Total messages received: {}",
        message_count
    );

    // Calculate messages per second based on first-to-last message duration
    if let (Some(first_msg_time), Some(last_msg_time)) = (first_message_time, last_message_time) {
        let message_duration = last_msg_time.duration_since(first_msg_time);
        let duration_seconds = message_duration.as_secs_f64();
        
        if message_count > 1 && duration_seconds > 0.0 {
            let messages_per_second = (message_count - 1) as f64 / duration_seconds;
            info!(
                "Performance: {:.2} messages/second over {:.2} seconds (excluding timeout)",
                messages_per_second, duration_seconds
            );
        } else if message_count == 1 {
            info!("Performance: Only 1 message received, no rate calculation possible");
        }
    } else if message_count > 0 {
        info!("Performance: Unable to calculate rate - timing data incomplete");
    }

    // Write message IDs to file if enabled
    if args.output && !message_ids.is_empty() {
        info!(
            "Writing {} message IDs to {}",
            message_ids.len(),
            args.output_file
        );
        let mut file = File::create(&args.output_file)?;
        for id in &message_ids {
            writeln!(file, "{}", id)?;
        }
        info!("Message IDs written to {}", args.output_file);
    } else if args.output {
        info!("No messages received, not creating output file");
    }

    info!("Stream monitor finished");
    Ok(())
}
