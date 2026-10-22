use super::*;
use crate::{
    config::{Config, ValidatedConfig},
    db::{PushChannel, Store},
    test_support::{TestDatabase, TestResult, config_for},
};
use channel::{Delivery, DeliveryConfig, Outcome, Sender, Senders};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Notify, Semaphore};
use xmtp_common::time::{Duration, Instant, timeout};
use xmtp_cryptography::secret::Zeroizing;

mod keyed;
mod lifecycle;
mod maintenance;
mod polling;
mod providers;
mod telemetry;
mod window;

struct Fixture {
    store: Store,
    /// The file layer. A test changes it, and each hub validates it.
    config: Config,
    _database: TestDatabase,
}

impl Fixture {
    async fn new() -> TestResult<Self> {
        let database = TestDatabase::new()?;
        let mut config = config_for(database.url());
        config.streams.poll_interval_ms = 25;
        config.push.max_attempts = 3;
        let store = Store::connect(&config.clone().validate()?).await?;
        Ok(Self {
            store,
            config,
            _database: database,
        })
    }

    async fn seed(&self, count: i64, topic: &[u8], commit: bool) -> TestResult<Vec<i64>> {
        Ok(sqlx::query_scalar("WITH ids AS MATERIALIZED (SELECT nextval('envelope_sequence') AS id FROM generate_series(1, $1)) INSERT INTO envelopes (sequence_id, topic, server_ns, message_hash, is_commit_or_proposal, payload, push_eligible) SELECT id, $2, 0, decode(md5(id::text) || md5(id::text), 'hex'), $3, ''::bytea, true FROM ids RETURNING sequence_id")
            .bind(count).bind(topic).bind(commit).fetch_all(&self.store.primary).await?)
    }

    async fn boundary(&self, sequence: i64) -> TestResult {
        sqlx::query("UPDATE allocation_boundary SET closed_sequence_id = $1 WHERE singleton")
            .bind(sequence)
            .execute(&self.store.primary)
            .await?;
        Ok(())
    }

    async fn recipient(
        &self,
        id: u64,
        channel: PushChannel,
        topic: &[u8],
        commits: bool,
        since: i64,
    ) -> TestResult<DeliveryConfig> {
        let mut recipient_id = vec![0; 32];
        recipient_id[24..].copy_from_slice(&id.to_be_bytes());
        let config = DeliveryConfig {
            recipient_id,
            secret_hash: vec![9; 32],
            channel,
            delivery: "https://push.invalid/hook".into(),
            signing_key: Some(Zeroizing::new(vec![7; 32])),
        };
        sqlx::query("INSERT INTO push_recipient (recipient_id, secret_hash, channel, delivery, signing_key, topic_count, renewed_ns) VALUES ($1, $2, $3, $4, $5, 1, (extract(epoch FROM clock_timestamp()) * 1000000000)::bigint)")
            .bind(&config.recipient_id).bind(&config.secret_hash).bind(channel as i16).bind(&config.delivery).bind(config.signing_key.as_ref().map(|key| key.as_slice()))
            .execute(&self.store.primary).await?;
        sqlx::query("INSERT INTO push_subscription (recipient_id, topic, since_sequence_id, include_commits) VALUES ($1, $2, $3, $4)")
            .bind(&config.recipient_id).bind(topic).bind(since).bind(commits).execute(&self.store.primary).await?;
        Ok(config)
    }

    async fn cursor(&self) -> i64 {
        sqlx::query_scalar("SELECT sequence_id FROM push_cursor WHERE singleton")
            .fetch_one(&self.store.primary)
            .await
            .unwrap()
    }

    /// The fixture's configuration as it is now, validated.
    fn validated(&self) -> ValidatedConfig {
        self.config
            .clone()
            .validate()
            .expect("the fixture configuration is valid")
    }

    fn hub(&self, sender: Arc<dyn Sender>) -> Arc<PushHub> {
        self.hub_with(sender.clone(), sender, Arc::new(Notify::new()))
    }

    fn hub_with(
        &self,
        http: Arc<dyn Sender>,
        other: Arc<dyn Sender>,
        maintenance: Arc<Notify>,
    ) -> Arc<PushHub> {
        PushHub::with_senders(
            self.store.clone(),
            &self.validated(),
            maintenance,
            Senders([Some(other), None, Some(http)]),
        )
    }
}

#[derive(Default)]
struct FakeSender {
    calls: Mutex<Vec<(u64, PushChannel)>>,
    /// The recipient of each call, in call order.
    recipients: Mutex<Vec<Vec<u8>>>,
    gate: Option<Arc<Semaphore>>,
    outcomes: Mutex<VecDeque<Outcome>>,
}

impl FakeSender {
    fn blocked() -> Arc<Self> {
        Arc::new(Self {
            gate: Some(Arc::new(Semaphore::new(0))),
            ..Self::default()
        })
    }

    fn count(&self) -> usize {
        self.calls.lock().len()
    }
}

#[async_trait::async_trait]
impl Sender for FakeSender {
    async fn send(&self, delivery: &Delivery) -> Outcome {
        self.calls.lock().push((
            delivery.payload.sequence_id.parse().unwrap(),
            delivery.config.channel,
        ));
        self.recipients
            .lock()
            .push(delivery.config.recipient_id.clone());
        if let Some(gate) = &self.gate {
            gate.acquire().await.unwrap().forget();
        }
        self.outcomes
            .lock()
            .pop_front()
            .unwrap_or(Outcome::Delivered)
    }
}

async fn stop(hub: &PushHub) {
    hub.stop(Instant::now() + Duration::from_secs(3));
    timeout(Duration::from_secs(4), hub.finished())
        .await
        .unwrap();
}

async fn holder_pid(store: &Store) -> Option<i32> {
    sqlx::query_scalar("SELECT pid FROM pg_locks WHERE locktype = 'advisory' AND classid = 0 AND objid = 3 AND objsubid = 2 AND granted AND database = (SELECT oid FROM pg_database WHERE datname = current_database())")
        .fetch_optional(&store.primary).await.unwrap()
}

/// A recipient whose expiry cannot be represented is an invariant failure. The
/// configuration's expiry arithmetic decides which renewals those are.
#[xmtp_common::test(unwrap_try = true)]
fn an_unrepresentable_recipient_expiry_is_an_invariant_failure() {
    let config: Config = toml::from_str(
        "[database]\nurl = 'postgres://localhost/xmtp'\n[push]\nrecipient_ttl_seconds = 86400",
    )?;
    assert_eq!(recipient_expiry(&config.push, 0)?, 86_400 * 1_000_000_000);
    for renewed_ns in [-1, i64::MAX] {
        assert!(matches!(
            recipient_expiry(&config.push, renewed_ns),
            Err(crate::error::Error::PushExpiryOverflow)
        ));
    }
}
