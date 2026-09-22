use super::*;
use crate::{
    config::Config,
    db::{PushChannel, Store},
    test_support::{TestDatabase, TestResult},
};
use channel::{Delivery, DeliveryConfig, Outcome, Sender, Senders};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Notify, Semaphore};
use xmtp_common::time::{Duration, Instant, timeout};

mod lifecycle;
mod maintenance;
mod polling;
mod providers;
mod telemetry;
mod window;

struct Fixture {
    store: Store,
    config: Config,
    _database: TestDatabase,
}

impl Fixture {
    async fn new() -> TestResult<Self> {
        let database = TestDatabase::new()?;
        let mut config: Config =
            toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
        config.streams.poll_interval_ms = 25;
        config.push.max_attempts = 3;
        let store = Store::connect(&config).await?;
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
            signing_key: Some(vec![7; 32]),
        };
        sqlx::query("INSERT INTO push_recipient (recipient_id, secret_hash, channel, delivery, signing_key, topic_count, renewed_ns) VALUES ($1, $2, $3, $4, $5, 1, (extract(epoch FROM clock_timestamp()) * 1000000000)::bigint)")
            .bind(&config.recipient_id).bind(&config.secret_hash).bind(channel as i16).bind(&config.delivery).bind(&config.signing_key)
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
            &self.config,
            maintenance,
            Senders([Some(other), None, Some(http)]),
        )
    }
}

#[derive(Default)]
struct FakeSender {
    calls: Mutex<Vec<(u64, PushChannel)>>,
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
