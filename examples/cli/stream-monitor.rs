#![recursion_limit = "256"]

use alloy::signers::local::PrivateKeySigner;
use clap::Parser;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{timeout, Duration};
use tracing::{error, info_span, Instrument};
use tracing_flame::FlameLayer;
use tracing_subscriber::{prelude::*, registry::Registry};
use xmtp_api_grpc::Client as GrpcApiClient;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::{EncryptedMessageStore, NativeDb, StorageOption};
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::Client;
use xmtp_mls::InboxOwner;

fn setup_global_subscriber(enable_fmt: bool) -> impl Drop {
    let l = std::env::var("TRACE_LEVEL").unwrap_or_else(|_| "trace".to_string());
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| format!("openmls={l},xmtp_api={l},xmtp_mls={l},xmtp_api_grpc={l},xmtp_id={l},xmtp_common={l},xmtp_db={l},xmtp_content_types={l},xmtp_cryptography={l},xmtp_proto={l},xmtp_configuration={l},openmls_rust_crypt={l},info"));
    let filter = tracing_subscriber::EnvFilter::builder()
        .parse(&env_filter)
        .unwrap();
    if enable_fmt {
        let file = File::create(
            std::env::var("RUST_LOG_FILE").unwrap_or_else(|_| "./tracing.log".to_string()),
        )
        .unwrap();
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_thread_names(true)
            .with_ansi(false)
            .with_target(true)
            .with_writer(file);
        let (flame_layer, _guard) = FlameLayer::with_file("./tracing.folded").unwrap();
        let flame_layer = flame_layer.with_threads_collapsed(true);
        let subscriber = Registry::default()
            .with(filter)
            .with(fmt_layer)
            .with(flame_layer);
        tracing::subscriber::set_global_default(subscriber).expect("Could not set global default");
        _guard
    } else {
        let (flame_layer, _guard) = FlameLayer::with_file("./tracing.folded").unwrap();
        let flame_layer = flame_layer.with_threads_collapsed(true);
        let subscriber = Registry::default().with(filter).with(flame_layer);
        tracing::subscriber::set_global_default(subscriber).expect("Could not set global default");
        _guard
    }
}

#[derive(Parser)]
#[command(name = "stream-monitor")]
#[command(about = "XMTP Stream Monitor - Listens for all messages using stream_all_messages")]
struct Args {
    /// Optional timeout in seconds for stream monitoring
    #[arg(long, default_value_t = 10)]
    timeout: u64,

    /// Output file for message IDs
    #[arg(long, default_value = "streamed-message-ids.txt")]
    output_file: String,

    /// Maximum number of messages to receive before ending
    #[arg(long, default_value_t = 10000)]
    max_messages: usize,

    /// Maximum number of messages to send before ending
    #[arg(long, default_value_t = 1000)]
    max_messages_per_group: usize,

    /// Use persistent database storage instead of ephemeral storage
    #[arg(long, default_value_t = false)]
    use_database: bool,

    /// Save message IDs to output file
    #[arg(long, default_value_t = false)]
    output: bool,

    /// Output file for all message IDs from all groups (after sync)
    #[arg(long, default_value = "received-message-ids.txt")]
    all_groups_output_file: String,

    /// Enable console logging output
    #[arg(long, default_value_t = false)]
    enable_logging: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let _guard = setup_global_subscriber(args.enable_logging);

    println!("Starting XMTP Stream Monitor");
    if args.enable_logging {
        println!("Logging enabled - console output will include trace logs");
    }

    // Create XMTP client
    println!("Creating XMTP client...");
    let client = create_client_with_wallet().await?;

    println!("Client created and registered successfully!");
    println!("Inbox ID: {}", client.inbox_id());

    // Start streaming all messages
    println!("Starting to stream all messages...");
    let mut message_stream = client.stream_all_messages(None, None).await?;

    let span = info_span!("stream_monitor.next");
    let mut message_ids = Vec::with_capacity(args.max_messages); // Pre-allocate capacity
    let mut message_count = 0;

    // Timeout will start when first message is received and reset on each new message
    let timeout_duration = Duration::from_secs(args.timeout);
    let mut first_message_time: Option<std::time::Instant> = None;
    let mut last_message_time: Option<std::time::Instant> = None;

    // Track min/max rates and recent message counts for accurate rate calculation
    let mut min_rate: Option<f64> = None;
    let mut max_rate: Option<f64> = None;
    let mut last_rate_update = Instant::now();
    let mut recent_message_count = 0;
    let mut last_message_count = 0;

    // Create progress bar
    let progress = ProgressBar::new(args.max_messages as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | {msg}")
            .expect("Failed to set progress bar template")
            .progress_chars("#>-")
    );
    progress.set_message(format!(
        "Waiting for messages... (timeout: {}s after last message)",
        args.timeout
    ));

    let sent_messages = tokio::spawn(send_messages(
        client.inbox_id().to_string(),
        args.max_messages,
        args.max_messages_per_group,
        args.timeout,
    ));

    loop {
        let span = span.clone();
        // Check for message limit first (fastest check)
        if message_count >= args.max_messages {
            println!("Message limit reached: {} messages", args.max_messages);
            break;
        }

        // Handle timeout logic based on whether we've received messages
        if let Some(last_msg_time) = last_message_time {
            // We've received at least one message, apply timeout logic
            let elapsed = last_msg_time.elapsed();
            if elapsed >= timeout_duration {
                println!(
                    "Timeout reached: {} seconds since last message",
                    args.timeout
                );
                break;
            }

            // Calculate remaining timeout and poll with timeout
            let remaining_timeout = timeout_duration - elapsed;
            match timeout(
                remaining_timeout,
                message_stream.next().instrument(span.clone()),
            )
            .await
            {
                Ok(Some(Ok(message))) => {
                    message_count += 1;
                    let now = Instant::now();

                    // Reset timeout timer on each message
                    last_message_time = Some(now);

                    // Store message ID
                    message_ids.push(hex::encode(&message.id));

                    // Update progress bar with rate calculation
                    progress.inc(1);
                    recent_message_count += 1;

                    // Calculate instantaneous rate every second
                    if now.duration_since(last_rate_update).as_secs_f64() >= 1.0 {
                        let time_window = now.duration_since(last_rate_update).as_secs_f64();
                        let messages_in_window = recent_message_count - last_message_count;
                        let current_rate = messages_in_window as f64 / time_window;

                        // Update min/max rates
                        if current_rate > 0.0 {
                            min_rate =
                                Some(min_rate.map_or(current_rate, |min| min.min(current_rate)));
                            max_rate =
                                Some(max_rate.map_or(current_rate, |max| max.max(current_rate)));
                        }

                        progress.set_message(format!("{:.1} msg/s", current_rate));

                        // Reset for next window
                        last_rate_update = now;
                        last_message_count = recent_message_count;
                    } else if first_message_time.is_none() {
                        progress.set_message("First message received!");
                    }
                }
                Ok(Some(Err(e))) => {
                    error!("Error receiving message: {}", e);
                    continue;
                }
                Ok(None) => {
                    println!("Stream ended");
                    break;
                }
                Err(_) => {
                    // Timeout occurred after receiving messages
                    println!(
                        "Timeout reached: {} seconds since last message",
                        args.timeout
                    );
                    break;
                }
            }
        } else {
            // No messages received yet, wait indefinitely for first message
            match message_stream.next().instrument(span.clone()).await {
                Some(Ok(message)) => {
                    message_count += 1;
                    let now = Instant::now();

                    // Start timer on first message
                    first_message_time = Some(now);
                    last_message_time = Some(now);
                    last_rate_update = now;

                    // Store message ID
                    message_ids.push(hex::encode(&message.id));

                    // Update progress bar for first message
                    progress.inc(1);
                    recent_message_count += 1;
                    progress.set_message("First message received! Timer started.");
                }
                Some(Err(e)) => {
                    error!("Error receiving message: {}", e);
                    continue;
                }
                None => {
                    println!("Stream ended");
                    break;
                }
            }
        }
    }

    drop(span);

    // Finish progress bar
    progress.finish_with_message(format!("Completed: {} messages received", message_count));

    let sent_messages = sent_messages.await??;
    println!("Sent {} messages", sent_messages.len());

    println!(
        "Stream monitoring completed. Total messages received: {}",
        message_count
    );

    // Calculate messages per second based on first-to-last message duration
    if let (Some(first_msg_time), Some(last_msg_time)) = (first_message_time, last_message_time) {
        let message_duration = last_msg_time.duration_since(first_msg_time);
        let duration_seconds = message_duration.as_secs_f64();

        if message_count > 1 && duration_seconds > 0.0 {
            let messages_per_second = (message_count - 1) as f64 / duration_seconds;
            println!(
                "Performance: {:.2} messages/second over {:.2} seconds",
                messages_per_second, duration_seconds
            );

            // Log min/max rates if we have them
            if let (Some(min), Some(max)) = (min_rate, max_rate) {
                println!(
                    "Performance range: min {:.2} msg/s, max {:.2} msg/s",
                    min, max
                );
            }
        } else if message_count == 1 {
            println!("Performance: Only 1 message received, no rate calculation possible");
        }
    } else if message_count > 0 {
        println!("Performance: Unable to calculate rate - timing data incomplete");
    }

    if sent_messages.len() == message_ids.len() {
        println!("All messages received successfully");
        return Ok(());
    }

    // Write message IDs to file if enabled
    if args.output && !message_ids.is_empty() {
        println!(
            "Writing {} message IDs to {}",
            message_ids.len(),
            args.output_file
        );
        let mut file = File::create(&args.output_file)?;
        for id in &message_ids {
            writeln!(file, "{}", id)?;
        }
        println!("Message IDs written to {}", args.output_file);
    } else if args.output {
        println!("No messages received, not creating output file");
    }

    let span = tracing::trace_span!("syncing_stream_monitor");
    let _span = span.enter();

    // Sync all groups and collect all message IDs if output is enabled
    if args.output {
        println!("Syncing all groups and collecting message IDs...");

        // Sync all welcomes and groups
        client
            .sync_all_welcomes_and_groups(None)
            .instrument(span.clone())
            .await?;

        // Get all groups
        let groups = client.find_groups(GroupQueryArgs::default()).unwrap();

        println!("Found {} groups to collect messages from", groups.len());

        let sent_message_ids = sent_messages
            .iter()
            .map(|m| m.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        // Collect all message IDs from all groups
        let mut all_message_ids = Vec::new();
        for group in groups {
            // Get all messages from this group
            let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();

            for message in messages {
                if message.kind == xmtp_db::group_message::GroupMessageKind::Application {
                    assert!(sent_message_ids.contains(hex::encode(&message.id).as_str()));
                    all_message_ids.push(message);
                } else {
                    println!(
                        "Skipping message ID {} of kind {:?}",
                        hex::encode(&message.id),
                        message.kind
                    );
                }
            }
        }

        if !all_message_ids.is_empty() {
            println!(
                "Writing {} total message IDs from all groups to {}",
                all_message_ids.len(),
                args.all_groups_output_file
            );
            let mut file = File::create(&args.all_groups_output_file)?;
            for message in &all_message_ids {
                writeln!(file, "{}", hex::encode(&message.id))?;
            }
            println!("All message IDs written to {}", args.all_groups_output_file);
            let received_message_ids_set = message_ids
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            println!(
                "Received {} message IDs out of {} total message IDs",
                received_message_ids_set.len(),
                all_message_ids.len()
            );
            for message in all_message_ids {
                if !received_message_ids_set.contains(&hex::encode(&message.id)) {
                    tracing::warn!(
                        message_id = %hex::encode(&message.id),
                        group_id = %hex::encode(&message.group_id),
                        decrypted_message_str = %String::from_utf8_lossy(&message.decrypted_message_bytes)[41..],
                        sent_at = %chrono::DateTime::from_timestamp_nanos(message.sent_at_ns),
                        sequence_id = message.sequence_id.unwrap_or(0),
                        "Message not found in received messages"
                    );
                }
            }
        } else {
            println!("No messages found in any groups");
        }
    }

    println!("Stream monitor finished");
    Err("Missing messages".into())
}

async fn send_messages(
    inbox_id: String,
    mut messages_to_send: usize,
    group_size: usize,
    timeout_seconds: u64,
) -> Result<Vec<SentMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let mut tasks = Vec::new();
    let mut message_ids = Vec::with_capacity(messages_to_send);
    let group_size = group_size.min(1000);
    while messages_to_send > 0 {
        let join = tokio::spawn(send_messages_in_thread(
            tasks.len() + 1,
            inbox_id.clone(),
            messages_to_send.min(group_size),
            timeout_seconds,
        ));
        tasks.push(join);
        messages_to_send = messages_to_send.saturating_sub(group_size);
    }
    for task in tasks {
        let mut sent_messages = task.await??;
        message_ids.append(&mut sent_messages);
    }
    Ok(message_ids)
}

struct SentMessage {
    id: String,
    text: String,
}

async fn send_messages_in_thread(
    task_id: usize,
    target_inbox_id: String,
    messages_to_send: usize,
    timeout_seconds: u64,
) -> Result<Vec<SentMessage>, Box<dyn std::error::Error + Send + Sync>> {
    use prost::Message;
    let client = create_client_with_wallet().await?;

    // Create or find DM with target inbox ID
    let group = client
        .find_or_create_dm_by_inbox_id(target_inbox_id.clone(), None)
        .await
        .expect("Failed to create DM");

    let mut message_ids = Vec::with_capacity(messages_to_send);

    let mut content_bytes = Vec::new();
    for i in 1..=messages_to_send {
        let text = format!("Message {i} from task {task_id}");

        // Encode message content
        let encoded_content =
            <xmtp_content_types::text::TextCodec as xmtp_content_types::ContentCodec<String>>::encode(text.clone()).unwrap();
        content_bytes.clear();
        encoded_content.encode(&mut content_bytes).unwrap();

        // Send message with timeout
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_seconds),
            group.send_message(&content_bytes),
        )
        .await
        {
            Ok(Ok(message_id)) => {
                let id = hex::encode(&message_id);

                // Store message ID
                message_ids.push(SentMessage { id, text });
            }
            Ok(Err(_e)) => {
                // Message send failed - continue without updating progress
            }
            Err(_) => {
                // Timeout occurred - continue without updating progress
            }
        }
    }

    Ok(message_ids)
}

type ClientType = Client<
    Arc<
        xmtp_mls::context::XmtpMlsLocalContext<
            Arc<GrpcApiClient>,
            EncryptedMessageStore<NativeDb>,
            xmtp_db::sql_key_store::SqlKeyStore<
                xmtp_db::DbConnection<
                    Arc<
                        xmtp_db::PersistentOrMem<
                            xmtp_db::NativeDbConnection,
                            xmtp_db::EphemeralDbConnection,
                        >,
                    >,
                >,
            >,
        >,
    >,
>;

async fn create_client_with_wallet() -> Result<ClientType, Box<dyn std::error::Error + Send + Sync>>
{
    // Generate a random wallet for signing
    let wallet = PrivateKeySigner::random();

    // Create XMTP client
    let nonce = 0;
    let ident = wallet.get_identifier().expect("Wallet address is invalid");
    let inbox_id = ident.inbox_id(nonce).expect("Failed to get inbox ID");

    // Create GRPC client for local node
    let api_client = Arc::new(
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
