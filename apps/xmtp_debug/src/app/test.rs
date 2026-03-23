//! Test scenarios for measuring XMTP performance metrics

use color_eyre::eyre::{Result, eyre};
use futures::stream::StreamExt;
use prost::Message as ProstMessage;
use std::time::Instant;
use xmtp_api_d14n::d14n::QueryEnvelopes;
use xmtp_configuration::Originators;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_proto::prelude::{ApiBuilder, NetConnectConfig, Query};
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::{
    app::{self, generate_wallet},
    args::{self, TestOpts, TestScenario},
    metrics::{self, record_phase_metric},
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
            TestScenario::MigrationLatency => self.migration_latency_test().await,
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
            )
            .await;
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
            )
            .await;
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

    // -----------------------------------------------------------------------
    // Migration Latency: V3 → V4
    // -----------------------------------------------------------------------

    /// Measures V3→V4 migration latency: write a message to V3, poll V4 until
    /// the migrator replicates it, and record the elapsed time.
    async fn migration_latency_test(&self) -> Result<()> {
        let v4_node_url = self
            .opts
            .v4_node_url
            .as_ref()
            .ok_or_else(|| eyre!("--v4-node-url is required for migration-latency scenario"))?;

        let iterations = self.opts.iterations;
        let timeout_secs = self.opts.migration_timeout;
        let mut latencies = Vec::with_capacity(iterations);

        info!(
            iterations,
            timeout_secs,
            v3_backend = ?self.network.backend,
            v4_node = %v4_node_url,
            "starting migration latency test"
        );

        // Build V4 query client once (reused across iterations).
        // Must point at a D14N replication node (grpc.testnet.xmtp.network),
        // NOT the payer gateway — the gateway doesn't serve QueryEnvelopes.
        let v4_client = {
            let mut builder = xmtp_api_grpc::GrpcClient::builder();
            builder.set_host(v4_node_url.clone());
            builder.build()?
        };
        info!(v4_node = %v4_node_url, "V4 query client ready");

        for i in 0..iterations {
            info!(iteration = i + 1, "running migration latency iteration");
            match self
                .run_single_migration_latency_test(&v4_client, timeout_secs)
                .await
            {
                Ok(latency) => {
                    latencies.push(latency);
                    let secs = latency as f64 / 1000.0;
                    metrics::record_migration_success();
                    metrics::record_migration_latency(secs);
                    metrics::push_metrics("xdbg_test").await;
                    info!(
                        iteration = i + 1,
                        latency_ms = latency,
                        "migration latency iteration complete"
                    );
                }
                Err(e) => {
                    metrics::record_migration_failure();
                    warn!(
                        iteration = i + 1,
                        error = %e,
                        "migration latency iteration failed"
                    );
                    metrics::push_metrics("xdbg_test").await;
                }
            }
        }

        // Print summary statistics
        if let (Some(&min), Some(&max)) = (latencies.iter().min(), latencies.iter().max()) {
            let sum: u128 = latencies.iter().sum();
            let avg = sum / latencies.len() as u128;
            let success_rate = latencies.len() as f64 / iterations as f64 * 100.0;

            info!(
                iterations,
                succeeded = latencies.len(),
                success_rate = format!("{:.1}%", success_rate),
                avg_ms = avg,
                min_ms = min,
                max_ms = max,
                "migration latency test summary"
            );
        } else {
            warn!(
                iterations,
                "all migration latency iterations failed — is the migrator running?"
            );
        }

        Ok(())
    }

    async fn run_single_migration_latency_test(
        &self,
        v4_client: &xmtp_api_grpc::GrpcClient,
        timeout_secs: u64,
    ) -> Result<u128> {
        // Step 1: Create a V3 SDK client
        info!("creating V3 sender identity");
        let wallet = generate_wallet();
        let client = app::temp_client(&self.network, Some(&wallet)).await?;
        app::register_client(&client, wallet.clone().into_alloy()).await?;
        info!(inbox_id = client.inbox_id(), "V3 sender registered");

        let result = self
            .do_migration_round_trip(&client, v4_client, timeout_secs)
            .await;
        client.release_db_connection()?;
        result
    }

    /// Runs a single V3→V4 migration round-trip. Caller is responsible for DB cleanup.
    async fn do_migration_round_trip(
        &self,
        client: &crate::DbgClient,
        v4_client: &xmtp_api_grpc::GrpcClient,
        timeout_secs: u64,
    ) -> Result<u128> {
        // Known migrator originator node IDs
        let migrator_originators: &[u32] = &[
            Originators::MLS_COMMITS,
            Originators::INBOX_LOG,
            Originators::APPLICATION_MESSAGES,
            Originators::WELCOME_MESSAGES,
            Originators::INSTALLATIONS,
        ];

        // Step 2: Create a group (just ourselves — sufficient for migration)
        let group = client.create_group(Default::default(), Default::default())?;
        let group_id = group.group_id.clone();
        let group_id_hex = hex::encode(&group_id);
        info!(group_id = group_id_hex, "group created on V3");

        // Step 3: Snapshot V4 baseline for this topic
        let topic = Topic::new_group_message(&group_id);
        let baseline_count = {
            let mut endpoint = QueryEnvelopes::builder()
                .envelopes(EnvelopesQuery {
                    topics: vec![topic.cloned_vec()],
                    originator_node_ids: vec![],
                    last_seen: None,
                })
                .limit(0u32) // 0 = no limit (server convention)
                .build()
                .map_err(|e| eyre!("build QueryEnvelopes: {e}"))?;
            match endpoint.query(v4_client).await {
                Ok(r) => r.envelopes.len(),
                Err(e) => {
                    debug!(error = %e, "V4 topic not yet available, assuming baseline 0");
                    0
                }
            }
        };
        info!(baseline_count, topic = group_id_hex, "V4 baseline");

        // Step 4: Send a tagged message on V3.
        // The tag encodes group_id + timestamp so we can verify the exact
        // envelope on V4, not just "any new envelope on this topic".
        let tag = format!(
            "__MIGRATION_MONITOR__{}_{}",
            group_id_hex,
            chrono::Utc::now().timestamp_millis()
        );
        info!(tag, "sending tagged message on V3");

        let start_time = Instant::now();
        group
            .send_message(
                tag.as_bytes(),
                SendMessageOptsBuilder::default()
                    .should_push(false)
                    .build()
                    .map_err(|e| eyre!("build SendMessageOpts: {e}"))?,
            )
            .await?;

        // After send_message + sync, the SDK has the V3 cursor for our message.
        // Extract it so we can match the exact (originator_id, sequence_id) on V4.
        let v3_messages = group.find_messages(&Default::default())?;
        let our_msg = v3_messages.iter().rev().find(|m| {
            String::from_utf8_lossy(&m.decrypted_message_bytes).contains("__MIGRATION_MONITOR__")
        });
        let (expected_originator, expected_sequence) = if let Some(msg) = our_msg {
            (msg.originator_id as u32, msg.sequence_id as u64)
        } else {
            (0, 0) // fallback: rely on count-based detection
        };

        info!(
            expected_originator,
            expected_sequence, "message sent on V3, starting V4 poll"
        );

        // Step 5: Poll V4 for the migrated envelope
        let poll_interval = std::time::Duration::from_millis(500);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let deadline = start_time + timeout;

        loop {
            if Instant::now() > deadline {
                return Err(eyre!(
                    "timeout after {}s waiting for message on V4 (topic={})",
                    timeout_secs,
                    group_id_hex
                ));
            }

            tokio::time::sleep(poll_interval).await;
            let elapsed = start_time.elapsed();

            let mut endpoint = QueryEnvelopes::builder()
                .envelopes(EnvelopesQuery {
                    topics: vec![topic.cloned_vec()],
                    originator_node_ids: vec![],
                    last_seen: None,
                })
                .limit(0u32) // 0 = no limit (server convention)
                .build()
                .map_err(|e| eyre!("build QueryEnvelopes: {e}"))?;

            let resp = match endpoint.query(v4_client).await {
                Ok(r) => r,
                Err(e) => {
                    debug!(elapsed_ms = elapsed.as_millis(), error = %e, "V4 query failed, retrying");
                    continue;
                }
            };

            // Look for our specific message by matching originator+sequence from V3.
            // The migrator preserves these IDs exactly.
            // Skip envelopes up to the baseline count to avoid matching pre-existing ones.
            for env in resp.envelopes.iter().skip(baseline_count) {
                let unsigned = match UnsignedOriginatorEnvelope::decode(
                    env.unsigned_originator_envelope.as_slice(),
                ) {
                    Ok(u) => u,
                    Err(_) => continue,
                };

                let matched = if expected_sequence > 0 {
                    // Exact match: same originator + sequence as V3
                    unsigned.originator_node_id == expected_originator
                        && unsigned.originator_sequence_id == expected_sequence
                } else {
                    // Fallback: any new envelope (beyond baseline) from a migrator originator
                    migrator_originators.contains(&unsigned.originator_node_id)
                };

                if matched {
                    let latency_ms = elapsed.as_millis();
                    info!(
                        latency_ms,
                        originator_id = unsigned.originator_node_id,
                        sequence_id = unsigned.originator_sequence_id,
                        is_migrator = migrator_originators.contains(&unsigned.originator_node_id),
                        topic = group_id_hex,
                        "message found on V4!"
                    );
                    return Ok(latency_ms);
                }
            }

            debug!(
                elapsed_ms = elapsed.as_millis(),
                total = resp.envelopes.len(),
                "target envelope not on V4 yet"
            );
        }
    }
}
