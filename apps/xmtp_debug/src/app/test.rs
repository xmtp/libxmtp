//! Test scenarios for measuring XMTP performance metrics

use color_eyre::eyre::{Result, eyre};
use futures::stream::StreamExt;
use prost::Message as ProstMessage;
use std::collections::HashSet;
use std::time::Instant;
use xmtp_api_d14n::d14n::QueryEnvelopes;
use xmtp_configuration::Originators;
use xmtp_db::encrypted_store::group_message::GroupMessageKind;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_proto::prelude::{ApiBuilder, NetConnectConfig, Query};
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::xmtpv4::envelopes::{OriginatorEnvelope, UnsignedOriginatorEnvelope};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

use crate::{
    app::{self, generate_wallet},
    args::{self, TestOpts, TestScenario},
    metrics::{self, record_phase_metric},
};

/// Tag embedded in parity test messages for identification in V3 local store.
const PARITY_TAG: &str = "__PARITY_CHECK__";

/// Tag embedded in continuity test messages for identification in V3 local store.
const CONTINUITY_TAG: &str = "__WALLET_CONTINUITY__";

/// Build a V4 query client from a URL.
/// Must point at a D14N replication node (e.g. grpc.testnet.xmtp.network),
/// NOT the payer gateway — the gateway doesn't serve QueryEnvelopes.
fn build_v4_client(url: &url::Url) -> Result<xmtp_api_grpc::GrpcClient> {
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host(url.clone());
    Ok(builder.build()?)
}

/// Fetch all envelopes for a topic from V4, returning an empty vec on error.
async fn query_v4_envelopes(
    v4_client: &xmtp_api_grpc::GrpcClient,
    topic: &Topic,
) -> Vec<OriginatorEnvelope> {
    let mut endpoint = match QueryEnvelopes::builder()
        .envelopes(EnvelopesQuery {
            topics: vec![topic.cloned_vec()],
            originator_node_ids: vec![],
            last_seen: None,
        })
        .limit(0u32)
        .build()
    {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    match endpoint.query(v4_client).await {
        Ok(r) => r.envelopes,
        Err(e) => {
            debug!(error = %e, "V4 query failed");
            vec![]
        }
    }
}

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
            TestScenario::ContentParity => self.content_parity_test().await,
            TestScenario::WalletContinuity => self.wallet_continuity_test().await,
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

        let v4_client = build_v4_client(v4_node_url)?;
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
                    metrics::push_metrics("xdbg_migration").await;
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
                    metrics::push_metrics("xdbg_migration").await;
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

    // -----------------------------------------------------------------------
    // Content Parity: V3 → V4
    // -----------------------------------------------------------------------

    /// Validates V3→V4 content parity: write structured payloads to V3, read
    /// back from V4, and diff content/count/ordering for each data type.
    async fn content_parity_test(&self) -> Result<()> {
        let v4_node_url = self
            .opts
            .v4_node_url
            .as_ref()
            .ok_or_else(|| eyre!("--v4-node-url is required for content-parity scenario"))?;

        let iterations = self.opts.iterations;
        let msg_count = self.opts.parity_messages;
        let timeout_secs = self.opts.migration_timeout;

        info!(
            iterations,
            msg_count,
            timeout_secs,
            v3_backend = ?self.network.backend,
            v4_node = %v4_node_url,
            "starting content parity test"
        );

        let v4_client = build_v4_client(v4_node_url)?;

        for i in 0..iterations {
            info!(iteration = i + 1, "running content parity iteration");
            match self.run_single_parity_test(&v4_client).await {
                Ok(()) => {
                    info!(iteration = i + 1, "content parity iteration complete");
                }
                Err(e) => {
                    warn!(iteration = i + 1, error = %e, "content parity iteration failed");
                }
            }
            metrics::push_metrics("xdbg_parity").await;
        }

        Ok(())
    }

    async fn run_single_parity_test(&self, v4_client: &xmtp_api_grpc::GrpcClient) -> Result<()> {
        let msg_count = self.opts.parity_messages;
        let timeout_secs = self.opts.migration_timeout;

        // Step 1: Create 2 V3 clients (sender + receiver)
        info!("creating sender");
        let wallet1 = generate_wallet();
        let client1 = app::temp_client(&self.network, Some(&wallet1)).await?;
        app::register_client(&client1, wallet1.clone().into_alloy()).await?;
        let inbox_id1 = client1.inbox_id().to_string();
        let install_id1 = client1.installation_public_key();
        info!(inbox_id = inbox_id1, "sender registered");

        info!("creating receiver");
        let wallet2 = generate_wallet();
        let client2 = app::temp_client(&self.network, Some(&wallet2)).await?;
        app::register_client(&client2, wallet2.clone().into_alloy()).await?;
        let inbox_id2 = client2.inbox_id().to_string();
        let install_id2 = client2.installation_public_key();
        info!(inbox_id = inbox_id2, "receiver registered");

        // Step 2: Snapshot V4 baselines BEFORE group operations (concurrent)
        let identity_topic1 = Topic::new_identity_update(&hex::decode(&inbox_id1)?);
        let identity_topic2 = Topic::new_identity_update(&hex::decode(&inbox_id2)?);
        let kp_topic1 = Topic::new_key_package(install_id1);
        let kp_topic2 = Topic::new_key_package(install_id2);
        let welcome_topic2 = Topic::new_welcome_message(install_id2);

        let (id_baseline1, id_baseline2, kp_baseline1, kp_baseline2, welcome_baseline2) = tokio::join!(
            async { query_v4_envelopes(v4_client, &identity_topic1).await.len() },
            async { query_v4_envelopes(v4_client, &identity_topic2).await.len() },
            async { query_v4_envelopes(v4_client, &kp_topic1).await.len() },
            async { query_v4_envelopes(v4_client, &kp_topic2).await.len() },
            async { query_v4_envelopes(v4_client, &welcome_topic2).await.len() },
        );

        info!(
            id_baseline1,
            id_baseline2, kp_baseline1, kp_baseline2, welcome_baseline2, "V4 baselines captured"
        );

        // Step 3: Sender creates group, adds receiver
        info!("creating group and adding receiver");
        let group = client1.create_group(Default::default(), Default::default())?;
        group.add_members(std::slice::from_ref(&inbox_id2)).await?;
        let group_id = group.group_id.clone();
        let group_id_hex = hex::encode(&group_id);
        info!(group_id = group_id_hex, "group created, receiver added");

        let group_topic = Topic::new_group_message(&group_id);
        let group_baseline = query_v4_envelopes(v4_client, &group_topic).await.len();
        info!(group_baseline, "group topic baseline");

        // Step 4: Send N tagged messages
        info!(msg_count, "sending tagged messages on V3");
        let send_start = Instant::now();
        for i in 0..msg_count {
            let payload = format!(
                "{}{}_{}_{}",
                PARITY_TAG,
                group_id_hex,
                i,
                chrono::Utc::now().timestamp_millis()
            );
            group
                .send_message(
                    payload.as_bytes(),
                    SendMessageOptsBuilder::default()
                        .should_push(false)
                        .build()
                        .map_err(|e| eyre!("build SendMessageOpts: {e}"))?,
                )
                .await?;
        }
        info!(
            elapsed_ms = send_start.elapsed().as_millis(),
            "all messages sent on V3"
        );

        // Extract V3 cursor data for our application messages
        let v3_messages = group.find_messages(&Default::default())?;
        let v3_cursors: Vec<(u32, u64)> = v3_messages
            .iter()
            .filter(|m| m.kind == GroupMessageKind::Application)
            .filter(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).contains(PARITY_TAG))
            .map(|m| (m.originator_id as u32, m.sequence_id as u64))
            .collect();
        info!(
            v3_count = v3_cursors.len(),
            expected = msg_count,
            "V3 application messages extracted"
        );

        // Step 5: Poll V4 until all messages appear (or timeout).
        // Keep the final envelope list to avoid a redundant re-fetch.
        let poll_interval = std::time::Duration::from_millis(500);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let deadline = Instant::now() + timeout;

        info!("polling V4 for migrated group messages...");
        let group_envelopes = loop {
            if Instant::now() > deadline {
                warn!(
                    timeout_secs,
                    group_id = group_id_hex,
                    "timeout waiting for group messages on V4"
                );
                break query_v4_envelopes(v4_client, &group_topic).await;
            }
            tokio::time::sleep(poll_interval).await;
            let envelopes = query_v4_envelopes(v4_client, &group_topic).await;
            let new_count = envelopes.len().saturating_sub(group_baseline);
            if new_count >= msg_count {
                info!(
                    new_envelopes = new_count,
                    total = envelopes.len(),
                    elapsed_ms = (Instant::now() - (deadline - timeout)).as_millis(),
                    "sufficient envelopes on V4"
                );
                break envelopes;
            }
        };

        // Step 6: Run parity checks

        // 6a: Group messages (uses already-fetched envelopes, no extra network call)
        Self::check_group_message_parity(
            &group_envelopes,
            group_baseline,
            &v3_cursors,
            &group_id_hex,
        )
        .await;

        // 6b-d: Identity updates, welcome messages, key packages (concurrent)
        let sender_label = format!("sender {}", &inbox_id1[..8]);
        let receiver_label = format!("receiver {}", &inbox_id2[..8]);
        tokio::join!(
            Self::check_topic_presence(
                v4_client,
                &identity_topic1,
                id_baseline1,
                "identity_updates",
                &sender_label,
                timeout_secs,
            ),
            Self::check_topic_presence(
                v4_client,
                &identity_topic2,
                id_baseline2,
                "identity_updates",
                &receiver_label,
                timeout_secs,
            ),
            Self::check_topic_presence(
                v4_client,
                &welcome_topic2,
                welcome_baseline2,
                "welcome_messages",
                &receiver_label,
                timeout_secs,
            ),
            Self::check_topic_presence(
                v4_client,
                &kp_topic1,
                kp_baseline1,
                "key_packages",
                &sender_label,
                timeout_secs,
            ),
            Self::check_topic_presence(
                v4_client,
                &kp_topic2,
                kp_baseline2,
                "key_packages",
                &receiver_label,
                timeout_secs,
            ),
        );

        // Clean up
        client1.release_db_connection()?;
        client2.release_db_connection()?;

        Ok(())
    }

    /// Check group message parity: match V3 cursors against V4 envelopes.
    /// Takes pre-fetched envelopes to avoid a redundant network call.
    async fn check_group_message_parity(
        envelopes: &[OriginatorEnvelope],
        baseline: usize,
        v3_cursors: &[(u32, u64)],
        group_id_hex: &str,
    ) {
        let data_type = "group_messages";
        let new_envelopes = &envelopes[baseline.min(envelopes.len())..];

        // Decode all new envelopes to get (originator_id, sequence_id) pairs
        let mut v4_cursors: Vec<(u32, u64)> = Vec::new();
        for env in new_envelopes {
            if let Ok(unsigned) =
                UnsignedOriginatorEnvelope::decode(env.unsigned_originator_envelope.as_slice())
            {
                v4_cursors.push((unsigned.originator_node_id, unsigned.originator_sequence_id));
            }
        }

        // Check 1: Every V3 cursor should exist in V4
        let v4_set: HashSet<(u32, u64)> = v4_cursors.iter().copied().collect();
        let mut matched = 0usize;
        let mut missing = 0usize;
        for cursor in v3_cursors {
            if v4_set.contains(cursor) {
                matched += 1;
            } else {
                missing += 1;
                warn!(
                    originator_id = cursor.0,
                    sequence_id = cursor.1,
                    group_id = group_id_hex,
                    "V3 message MISSING on V4"
                );
            }
        }

        // Check 2: Ordering — V4 sequence IDs should be monotonically non-decreasing
        // within the same originator
        let mut ordering_ok = true;
        let mut last_seq_by_originator: std::collections::HashMap<u32, u64> =
            std::collections::HashMap::new();
        for &(orig, seq) in &v4_cursors {
            if let Some(&last) = last_seq_by_originator.get(&orig)
                && seq < last
            {
                ordering_ok = false;
                warn!(
                    originator_id = orig,
                    current_seq = seq,
                    previous_seq = last,
                    "V4 ordering violation"
                );
            }
            last_seq_by_originator.insert(orig, seq);
        }

        // Check 3: No duplicates
        let unique_count = v4_set.len();
        let duplicate_count = v4_cursors.len().saturating_sub(unique_count);
        if duplicate_count > 0 {
            warn!(duplicate_count, "duplicate envelopes on V4");
            metrics::record_parity_extra(data_type, duplicate_count as u64);
        }

        let passed = missing == 0 && ordering_ok && duplicate_count == 0;
        if passed {
            metrics::record_parity_pass(data_type);
            info!(
                matched,
                v3_count = v3_cursors.len(),
                v4_new = v4_cursors.len(),
                ordering_ok,
                group_id = group_id_hex,
                "group message parity: PASS"
            );
        } else {
            metrics::record_parity_fail(data_type);
            if missing > 0 {
                metrics::record_parity_missing(data_type, missing as u64);
            }
            warn!(
                matched,
                missing,
                duplicate_count,
                ordering_ok,
                v3_count = v3_cursors.len(),
                v4_new = v4_cursors.len(),
                group_id = group_id_hex,
                "group message parity: FAIL"
            );
        }

        record_phase_metric(
            "test_content_parity_group_messages",
            if passed { 1.0 } else { 0.0 },
            "content_parity",
            "xdbg_parity",
        )
        .await;
    }

    /// Poll V4 for at least one new envelope on a topic.
    /// Uses `timeout_secs / 4` as the presence timeout (secondary data types
    /// should already be migrated by the time group messages arrive).
    async fn check_topic_presence(
        v4_client: &xmtp_api_grpc::GrpcClient,
        topic: &Topic,
        baseline: usize,
        data_type: &str,
        label: &str,
        timeout_secs: u64,
    ) {
        let poll_interval = std::time::Duration::from_secs(2);
        // Use a quarter of the migration timeout for secondary topics —
        // they should migrate alongside or before group messages.
        let timeout = std::time::Duration::from_secs((timeout_secs / 4).max(4));
        let deadline = Instant::now() + timeout;

        loop {
            let count = query_v4_envelopes(v4_client, topic).await.len();
            let new_count = count.saturating_sub(baseline);

            if new_count > 0 {
                metrics::record_parity_pass(data_type);
                info!(
                    data_type,
                    label,
                    new_envelopes = new_count,
                    "{} parity: PASS",
                    data_type
                );
                return;
            }

            if Instant::now() > deadline {
                metrics::record_parity_fail(data_type);
                metrics::record_parity_missing(data_type, 1);
                warn!(
                    data_type,
                    label,
                    baseline,
                    current = count,
                    timeout_secs = timeout.as_secs(),
                    "{} parity: FAIL — no new envelopes on V4",
                    data_type
                );
                return;
            }

            tokio::time::sleep(poll_interval).await;
        }
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

    // -----------------------------------------------------------------------
    // Wallet Continuity: V3 → V4
    // -----------------------------------------------------------------------

    /// Validates wallet continuity across V3→V4 migration: create an identity
    /// and group on V3, wait for migration, then verify the same wallet can
    /// register on V4 and all group data is intact.
    async fn wallet_continuity_test(&self) -> Result<()> {
        let v4_node_url = self
            .opts
            .v4_node_url
            .as_ref()
            .ok_or_else(|| eyre!("--v4-node-url is required for wallet-continuity scenario"))?;

        let iterations = self.opts.iterations;
        let msg_count = self.opts.continuity_messages;
        let timeout_secs = self.opts.migration_timeout;

        info!(
            iterations,
            msg_count,
            timeout_secs,
            v3_backend = ?self.network.backend,
            v4_node = %v4_node_url,
            "starting wallet continuity test"
        );

        let v4_client = build_v4_client(v4_node_url)?;

        for i in 0..iterations {
            info!(iteration = i + 1, "running wallet continuity iteration");
            match self.run_single_continuity_test(&v4_client).await {
                Ok(()) => {
                    info!(iteration = i + 1, "wallet continuity iteration complete");
                }
                Err(e) => {
                    warn!(iteration = i + 1, error = %e, "wallet continuity iteration failed");
                }
            }
            metrics::push_metrics("xdbg_continuity").await;
        }

        Ok(())
    }

    async fn run_single_continuity_test(
        &self,
        v4_client: &xmtp_api_grpc::GrpcClient,
    ) -> Result<()> {
        let msg_count = self.opts.continuity_messages;
        let timeout_secs = self.opts.migration_timeout;

        // Step 1: Create V3 sender + receiver
        info!("creating V3 sender");
        let wallet1 = generate_wallet();
        let client1 = app::temp_client(&self.network, Some(&wallet1)).await?;
        app::register_client(&client1, wallet1.clone().into_alloy()).await?;
        let inbox_id1 = client1.inbox_id().to_string();
        info!(inbox_id = inbox_id1, "V3 sender registered");

        info!("creating V3 receiver");
        let wallet2 = generate_wallet();
        let client2 = app::temp_client(&self.network, Some(&wallet2)).await?;
        app::register_client(&client2, wallet2.clone().into_alloy()).await?;
        let inbox_id2 = client2.inbox_id().to_string();
        info!(inbox_id = inbox_id2, "V3 receiver registered");

        // Step 2: Sender creates group, adds receiver
        info!("creating group and adding receiver");
        let group = client1.create_group(Default::default(), Default::default())?;
        group.add_members(std::slice::from_ref(&inbox_id2)).await?;
        let group_id = group.group_id.clone();
        let group_id_hex = hex::encode(&group_id);
        info!(group_id = group_id_hex, "group created, receiver added");

        // Capture group topic baseline BEFORE sending messages
        let group_topic = Topic::new_group_message(&group_id);
        let group_baseline = query_v4_envelopes(v4_client, &group_topic).await.len();
        info!(group_baseline, "group topic baseline captured");

        // Step 3: Send N tagged messages
        info!(msg_count, "sending tagged messages on V3");
        for i in 0..msg_count {
            let payload = format!(
                "{}{}_{}_{}",
                CONTINUITY_TAG,
                group_id_hex,
                i,
                chrono::Utc::now().timestamp_millis()
            );
            group
                .send_message(
                    payload.as_bytes(),
                    SendMessageOptsBuilder::default()
                        .should_push(false)
                        .build()
                        .map_err(|e| eyre!("build SendMessageOpts: {e}"))?,
                )
                .await?;
        }
        info!("all V3 messages sent");

        // Step 4: Capture V3 member count
        group.sync().await?;
        let member_count = group.members().await?.len();
        info!(member_count, "V3 membership captured");

        // Step 5: Poll V4 for migrated group messages
        let poll_interval = std::time::Duration::from_millis(500);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let deadline = Instant::now() + timeout;

        info!("polling V4 for migrated group messages...");
        let group_envelopes = loop {
            if Instant::now() > deadline {
                warn!(timeout_secs, "timeout waiting for group messages on V4");
                break query_v4_envelopes(v4_client, &group_topic).await;
            }
            tokio::time::sleep(poll_interval).await;
            let envelopes = query_v4_envelopes(v4_client, &group_topic).await;
            let new_count = envelopes.len().saturating_sub(group_baseline);
            if new_count >= msg_count {
                info!(new_envelopes = new_count, "group messages migrated to V4");
                break envelopes;
            }
        };

        // Step 6: Create V4 SDK client with sender's wallet (D14N mode).
        // This tests the core identity continuity promise: same wallet → same
        // inbox_id, and the V4 network accepts a new installation for the
        // migrated identity.
        let d14n_backend = args::BackendOpts {
            backend: self.network.backend,
            d14n: true,
            ..Default::default()
        };

        info!("creating V4 SDK client with sender's wallet");
        let v4_sdk_client = app::temp_client(&d14n_backend, Some(&wallet1)).await?;
        let v4_inbox_id = v4_sdk_client.inbox_id().to_string();

        // CHECK 1: inbox_id derivation is deterministic across backends
        if v4_inbox_id == inbox_id1 {
            metrics::record_continuity_pass("identity_inbox_id");
            info!(
                inbox_id = v4_inbox_id,
                "identity inbox_id: PASS — matches V3"
            );
        } else {
            metrics::record_continuity_fail("identity_inbox_id");
            warn!(
                v3_inbox = inbox_id1,
                v4_inbox = v4_inbox_id,
                "identity inbox_id: FAIL — mismatch"
            );
        }

        // CHECK 2: V4 registration succeeds (new installation for migrated inbox)
        match app::register_client(&v4_sdk_client, wallet1.clone().into_alloy()).await {
            Ok(()) => {
                metrics::record_continuity_pass("identity_registration");
                info!("identity registration: PASS — V4 accepted new installation");
            }
            Err(e) => {
                metrics::record_continuity_fail("identity_registration");
                warn!(error = %e, "identity registration: FAIL — V4 rejected");
            }
        }

        // CHECK 3: Identity discoverability — both identities exist on V4
        let identity_topic1 = Topic::new_identity_update(&hex::decode(&inbox_id1)?);
        let identity_topic2 = Topic::new_identity_update(&hex::decode(&inbox_id2)?);
        // Identity updates should migrate alongside or before group messages, so
        // use 1/4 of the migration timeout with a 4s floor for the presence poll.
        let presence_timeout = std::time::Duration::from_secs((timeout_secs / 4).max(4));

        let (sender_found, receiver_found) = tokio::join!(
            Self::poll_topic_presence(v4_client, &identity_topic1, presence_timeout),
            Self::poll_topic_presence(v4_client, &identity_topic2, presence_timeout),
        );

        if sender_found && receiver_found {
            metrics::record_continuity_pass("membership");
            info!("membership: PASS — both identities discoverable on V4");
        } else {
            metrics::record_continuity_fail("membership");
            warn!(
                sender_found,
                receiver_found, "membership: FAIL — identity not found on V4"
            );
        }

        // CHECK 4: Message completeness — all N messages present on V4
        let new_envelopes = &group_envelopes[group_baseline.min(group_envelopes.len())..];
        let v4_count = new_envelopes.len();
        if v4_count >= msg_count {
            metrics::record_continuity_pass("message_completeness");
            info!(
                expected = msg_count,
                found = v4_count,
                "message completeness: PASS"
            );
        } else {
            metrics::record_continuity_fail("message_completeness");
            warn!(
                expected = msg_count,
                found = v4_count,
                "message completeness: FAIL — missing messages"
            );
        }

        // CHECK 5: Message ordering — sequence IDs monotonic per originator
        let mut ordering_ok = true;
        let mut last_seq_by_originator: std::collections::HashMap<u32, u64> =
            std::collections::HashMap::new();
        for env in new_envelopes {
            match UnsignedOriginatorEnvelope::decode(env.unsigned_originator_envelope.as_slice()) {
                Err(e) => {
                    debug!(error = %e, "envelope decode failed during ordering check");
                }
                Ok(unsigned) => {
                    let orig = unsigned.originator_node_id;
                    let seq = unsigned.originator_sequence_id;
                    if let Some(&last) = last_seq_by_originator.get(&orig)
                        && seq < last
                    {
                        ordering_ok = false;
                        warn!(
                            originator_id = orig,
                            current_seq = seq,
                            previous_seq = last,
                            "V4 ordering violation"
                        );
                    }
                    last_seq_by_originator.insert(orig, seq);
                }
            }
        }
        if ordering_ok {
            metrics::record_continuity_pass("message_ordering");
            info!("message ordering: PASS");
        } else {
            metrics::record_continuity_fail("message_ordering");
            warn!("message ordering: FAIL");
        }

        // Clean up
        v4_sdk_client.release_db_connection()?;
        client1.release_db_connection()?;
        client2.release_db_connection()?;

        Ok(())
    }

    /// Poll a V4 topic until at least one envelope exists, returning true if found.
    async fn poll_topic_presence(
        v4_client: &xmtp_api_grpc::GrpcClient,
        topic: &Topic,
        timeout: std::time::Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        let poll_interval = std::time::Duration::from_secs(2);
        loop {
            let count = query_v4_envelopes(v4_client, topic).await.len();
            if count > 0 {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            tokio::time::sleep(poll_interval).await;
        }
    }
}
