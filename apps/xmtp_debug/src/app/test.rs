//! Test scenarios for measuring XMTP performance metrics

use color_eyre::eyre::{Result, eyre};
use futures::stream::StreamExt;
use std::time::Instant;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;

use crate::{
    app::{self, generate_wallet},
    args::{self, TestOpts, TestScenario},
    metrics::record_phase_metric,
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
            TestScenario::GroupSync => self.group_sync_test().await,
        }
    }

    /// Measures message visibility latency - the time from when one client sends
    /// a message to a group chat until another client in the same group receives
    /// it via stream.
    async fn message_visibility_test(&self) -> Result<()> {
        let iterations = self.opts.iterations;
        let mut latencies = Vec::with_capacity(iterations);

        info!(
            iterations,
            backend = ?self.network.backend,
            "starting message visibility test"
        );

        for i in 0..iterations {
            info!(iteration = i + 1, "running iteration");
            let latency = self.run_single_visibility_test().await?;
            latencies.push(latency);
            record_phase_metric(
                "test_message_visibility_seconds",
                latency as f64 / 1000.0,
                "message_visibility",
                "xdbg_test",
            );
            info!(
                iteration = i + 1,
                latency_ms = latency,
                "iteration complete"
            );
        }

        // Print summary statistics
        if iterations > 1 {
            let sum: u128 = latencies.iter().sum();
            let avg = sum / iterations as u128;
            let min = *latencies.iter().min().unwrap();
            let max = *latencies.iter().max().unwrap();

            info!(
                iterations,
                avg_ms = avg,
                min_ms = min,
                max_ms = max,
                "message visibility test summary"
            );
        }

        Ok(())
    }

    async fn run_single_visibility_test(&self) -> Result<u128> {
        // Step 1: Create 2 fresh users/identities
        info!("creating sender");
        let wallet1 = generate_wallet();
        let client1 = app::temp_client(&self.network, Some(&wallet1)).await?;
        app::register_client(&client1, wallet1.clone().into_alloy()).await?;
        let inbox_id1 = client1.inbox_id().to_string();
        info!(inbox_id = inbox_id1, "sender created");

        info!("creating receiver");
        let wallet2 = generate_wallet();
        let client2 = app::temp_client(&self.network, Some(&wallet2)).await?;
        app::register_client(&client2, wallet2.clone().into_alloy()).await?;
        let inbox_id2 = client2.inbox_id().to_string();
        info!(inbox_id = inbox_id2, "receiver created");

        // Step 2: user1 creates a group chat and adds user2
        info!("creating group and adding receiver");
        let group = client1.create_group(Default::default(), Default::default())?;
        group.add_members(std::slice::from_ref(&inbox_id2)).await?;
        let group_id = hex::encode(&group.group_id);
        info!(group_id, "group created");

        // Sync user2 to receive the group welcome
        info!("syncing receiver welcomes");
        client2.sync_welcomes().await?;

        // Step 3: user2 starts listening to a message stream for the group
        info!("receiver starting message stream");
        let stream = client2.stream_all_messages(None, None).await?;
        tokio::pin!(stream);

        // Prepare the test message
        let test_message = format!("visibility_test_{}", chrono::Utc::now().timestamp_millis());

        // Step 4: user1 sends a message (record START TIME)
        info!("sender sending message");
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
        info!(message = test_message, "message sent");

        // Step 5: user2 receives the message via stream (record END TIME)
        info!("waiting for receiver to get message");

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
        info!(latency_ms, "message received by receiver");

        // Clean up
        client1.release_db_connection()?;
        client2.release_db_connection()?;

        Ok(latency_ms)
    }

    /// Measures group sync latency - the time for a client to sync and retrieve
    /// N messages that were sent while the client was offline.
    async fn group_sync_test(&self) -> Result<()> {
        let iterations = self.opts.iterations;
        let message_count = self.opts.message_count;
        let mut latencies = Vec::with_capacity(iterations);

        info!(
            iterations,
            message_count,
            backend = ?self.network.backend,
            "starting group sync test"
        );

        for i in 0..iterations {
            info!(iteration = i + 1, "running iteration");
            let latency = self.run_single_group_sync_test().await?;
            latencies.push(latency);
            record_phase_metric(
                "test_group_sync_seconds",
                latency as f64 / 1000.0,
                "group_sync",
                "xdbg_test",
            );
            info!(
                iteration = i + 1,
                sync_latency_ms = latency,
                "iteration complete"
            );
        }

        // Print summary statistics
        if iterations > 1 {
            let sum: u128 = latencies.iter().sum();
            let avg = sum / iterations as u128;
            let min = *latencies.iter().min().unwrap();
            let max = *latencies.iter().max().unwrap();

            info!(
                iterations,
                message_count,
                avg_ms = avg,
                min_ms = min,
                max_ms = max,
                "group sync test summary"
            );
        }

        Ok(())
    }

    async fn run_single_group_sync_test(&self) -> Result<u128> {
        let message_count = self.opts.message_count;

        // Step 1: Create 2 fresh users/identities
        info!("creating sender");
        let wallet1 = generate_wallet();
        let client1 = app::temp_client(&self.network, Some(&wallet1)).await?;
        app::register_client(&client1, wallet1.clone().into_alloy()).await?;
        let inbox_id1 = client1.inbox_id().to_string();
        info!(inbox_id = inbox_id1, "sender created");

        info!("creating receiver");
        let wallet2 = generate_wallet();
        let client2 = app::temp_client(&self.network, Some(&wallet2)).await?;
        app::register_client(&client2, wallet2.clone().into_alloy()).await?;
        let inbox_id2 = client2.inbox_id().to_string();
        info!(inbox_id = inbox_id2, "receiver created");

        // Step 2: user1 creates a group chat and adds user2
        info!("creating group and adding receiver");
        let group = client1.create_group(Default::default(), Default::default())?;
        group.add_members(std::slice::from_ref(&inbox_id2)).await?;
        let group_id = hex::encode(&group.group_id);
        info!(group_id, "group created");

        // Step 3: Sync user2 to receive the group welcome (but don't sync messages yet)
        info!("syncing receiver welcomes");
        let mut welcome_attempts = 0;
        loop {
            client2.sync_welcomes().await?;
            if client2.group(&group.group_id).is_ok() {
                break;
            }
            welcome_attempts += 1;
            if welcome_attempts >= 30 {
                return Err(eyre!(
                    "Welcome never arrived after {} attempts",
                    welcome_attempts
                ));
            }
            info!(
                attempt = welcome_attempts,
                "welcome not yet received, retrying"
            );
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        // Step 4: user1 sends N messages to the group
        info!(message_count, "sender sending messages");
        for i in 0..message_count {
            let msg = format!(
                "history-msg-{}-{}",
                i,
                chrono::Utc::now().timestamp_millis()
            );
            group
                .send_message(
                    msg.as_bytes(),
                    SendMessageOptsBuilder::default()
                        .should_push(false)
                        .build()
                        .unwrap(),
                )
                .await?;
        }
        info!("messages sent");

        // Step 5: user2 syncs the group and retrieves messages (measure this)
        info!("receiver syncing group");
        let receiver_group = client2.group(&group.group_id)?;

        let sync_start = Instant::now();
        receiver_group.sync().await?;
        let messages = receiver_group.find_messages(&Default::default())?;
        let sync_duration = sync_start.elapsed().as_millis();

        info!(
            synced_count = messages.len(),
            sync_ms = sync_duration,
            "receiver sync complete"
        );

        // Verify we got the expected messages (at least message_count)
        // Note: messages includes membership changes, so count may be higher
        if messages.len() < message_count {
            return Err(eyre!(
                "Expected at least {} messages, got {}",
                message_count,
                messages.len()
            ));
        }

        // Clean up
        client1.release_db_connection()?;
        client2.release_db_connection()?;

        Ok(sync_duration)
    }
}
