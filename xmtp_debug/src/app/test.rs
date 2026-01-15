//! Test scenarios for measuring XMTP performance metrics

use color_eyre::eyre::{Result, eyre};
use futures::stream::StreamExt;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;

use crate::{
    app::{self, generate_wallet},
    args::{self, TestOpts, TestScenario},
};

pub struct Test {
    opts: TestOpts,
    network: args::BackendOpts,
}

impl Test {
    pub fn new(opts: TestOpts, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        match self.opts.scenario {
            TestScenario::MessageVisibility => self.message_visibility_test().await,
        }
    }

    /// Measures message visibility latency - the time from when one client sends
    /// a message to a group chat until another client in the same group receives
    /// it via stream.
    async fn message_visibility_test(&self) -> Result<()> {
        let iterations = self.opts.iterations;
        let mut latencies = Vec::with_capacity(iterations);

        println!(
            "Running message visibility test with {} iteration(s)",
            iterations
        );
        println!("Network: {:?}", self.network.backend);
        println!();

        for i in 0..iterations {
            println!("--- Iteration {} ---", i + 1);
            let latency = self.run_single_visibility_test().await?;
            latencies.push(latency);
            println!("Latency: {} ms", latency);
            println!();
        }

        // Print summary statistics
        if iterations > 1 {
            let sum: u128 = latencies.iter().sum();
            let avg = sum / iterations as u128;
            let min = *latencies.iter().min().unwrap();
            let max = *latencies.iter().max().unwrap();

            println!("=== Summary ===");
            println!("Iterations: {}", iterations);
            println!("Average latency: {} ms", avg);
            println!("Min latency: {} ms", min);
            println!("Max latency: {} ms", max);
        }

        Ok(())
    }

    async fn run_single_visibility_test(&self) -> Result<u128> {
        // Step 1: Create 2 fresh users/identities
        println!("Creating user1 (sender)...");
        let wallet1 = generate_wallet();
        let client1 = app::temp_client(&self.network, Some(&wallet1)).await?;
        app::register_client(&client1, wallet1.clone().into_alloy()).await?;
        let inbox_id1 = client1.inbox_id().to_string();
        println!("  User1 inbox_id: {}", inbox_id1);

        println!("Creating user2 (receiver)...");
        let wallet2 = generate_wallet();
        let client2 = app::temp_client(&self.network, Some(&wallet2)).await?;
        app::register_client(&client2, wallet2.clone().into_alloy()).await?;
        let inbox_id2 = client2.inbox_id().to_string();
        println!("  User2 inbox_id: {}", inbox_id2);

        // Step 2: user1 creates a group chat and adds user2
        println!("User1 creating group and adding user2...");
        let group = client1.create_group(Default::default(), Default::default())?;
        group
            .add_members_by_inbox_id(std::slice::from_ref(&inbox_id2))
            .await?;
        let group_id = hex::encode(&group.group_id);
        println!("  Group created: {}", group_id);

        // Sync user2 to receive the group welcome
        println!("Syncing user2 welcomes...");
        client2.sync_welcomes().await?;

        // Step 3: user2 starts listening to a message stream for the group
        println!("User2 starting message stream...");
        let stream = client2.stream_all_messages(None, None).await?;
        tokio::pin!(stream);

        // Prepare the test message
        let test_message = format!("visibility_test_{}", chrono::Utc::now().timestamp_millis());

        // Step 4: user1 sends a message (record START TIME)
        println!("User1 sending message...");
        let start_time = Instant::now();
        group
            .send_message(
                test_message.as_bytes(),
                SendMessageOptsBuilder::default()
                    .should_push(true)
                    .build()
                    .unwrap(),
            )
            .await?;
        println!("  Message sent: {}", test_message);

        // Step 5: user2 receives the message via stream (record END TIME)
        println!("Waiting for user2 to receive message...");

        // Set a timeout to avoid hanging indefinitely
        let timeout_duration = std::time::Duration::from_secs(30);
        let receive_result = tokio::time::timeout(timeout_duration, async {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(message) => {
                        let content = String::from_utf8_lossy(&message.decrypted_message_bytes);
                        if content == test_message {
                            return Ok(Instant::now());
                        }
                    }
                    Err(e) => {
                        warn!("Error receiving message: {:?}", e);
                    }
                }
            }
            Err(eyre!("Stream ended without receiving the test message"))
        })
        .await;

        let end_time = match receive_result {
            Ok(Ok(time)) => time,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(eyre!(
                    "Timeout waiting for message ({}s)",
                    timeout_duration.as_secs()
                ));
            }
        };

        // Step 6: Report END TIME - START TIME as the "message visibility latency"
        let latency_ms = end_time.duration_since(start_time).as_millis();
        println!("  Message received by user2");

        // Clean up
        client1.release_db_connection()?;
        client2.release_db_connection()?;

        Ok(latency_ms)
    }
}
