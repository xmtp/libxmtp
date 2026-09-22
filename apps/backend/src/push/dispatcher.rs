//! Session-lock owner and bounded shutdown of the delivery pipeline.

use sqlx::{Connection, PgConnection};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{Notify, watch},
    task::{JoinHandle, JoinSet},
};
use xmtp_common::time::{Duration, Instant, sleep, timeout};

use super::{
    channel::{ATTEMPT_TIMEOUT, DeliveryConfig, Outcome, Senders},
    window,
    work::{Attempt, Completion, Work},
};
use crate::{
    config::Config,
    db::{self, Store},
    error::Error,
    telemetry,
};

const EXPIRY_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub(crate) struct PushHub {
    stop: watch::Sender<Option<Instant>>,
    worker: JoinHandle<()>,
    done: watch::Receiver<bool>,
}

impl PushHub {
    pub fn start(
        store: Store,
        config: &Config,
        maintenance: Arc<Notify>,
        senders: Senders,
    ) -> Arc<Self> {
        Self::with_senders(store, config, maintenance, senders)
    }

    pub(super) fn with_senders(
        store: Store,
        config: &Config,
        maintenance: Arc<Notify>,
        senders: Senders,
    ) -> Arc<Self> {
        let (stop, stopped) = watch::channel(None);
        let (finished, done) = watch::channel(false);
        let config = config.clone();
        let worker = tokio::spawn(async move {
            supervise(store, config, maintenance, senders, stopped).await;
            let _ = finished.send(true);
        });
        Arc::new(Self { stop, worker, done })
    }

    /// Stop loading immediately. The server supplies the same absolute deadline
    /// used for its RPC drain, so retries cannot extend shutdown.
    pub fn stop(&self, deadline: Instant) {
        self.stop.send_if_modified(|current| {
            if current.is_none_or(|old| deadline < old) {
                *current = Some(deadline);
                true
            } else {
                false
            }
        });
    }

    pub async fn finished(&self) {
        let mut done = self.done.clone();
        // Cancellation drops the only sender. Channel closure also means the
        // worker has exited, even though it could not publish true.
        let _ = done.wait_for(|done| *done).await;
    }

    /// End work after the server drain deadline or abrupt server cancellation.
    pub fn abort(&self) {
        self.worker.abort();
    }
}

impl Drop for PushHub {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

/// Check whether push needs the closed allocation boundary to advance.
/// The maximum uses the partial index even with a generic prepared plan.
#[xmtp_common::db_span]
pub(super) async fn has_unsettled_envelopes(store: &Store, boundary: i64) -> Result<bool, Error> {
    Ok(sqlx::query_file_scalar!("src/push/unsettled.sql", boundary)
        .fetch_one(&store.read)
        .await?)
}

struct HolderGauge;
impl Drop for HolderGauge {
    fn drop(&mut self) {
        telemetry::push_dispatcher(false);
    }
}

async fn supervise(
    store: Store,
    config: Config,
    maintenance: Arc<Notify>,
    senders: Senders,
    mut stop: watch::Receiver<Option<Instant>>,
) {
    telemetry::push_dispatcher(false);
    let interval = Duration::from_millis(config.streams.poll_interval_ms);
    loop {
        if stop.borrow().is_some() {
            return;
        }
        let acquired = async {
            let mut connection =
                db::dedicated_read(&store.primary, config.database.max_statement_timeout_ms)
                    .await?;
            let locked: bool = sqlx::query_scalar!(
                r#"SELECT pg_try_advisory_lock($1, $2) AS "locked!""#,
                db::GLOBAL_LOCK_DOMAIN,
                db::PUSH_DISPATCHER
            )
            .fetch_one(&mut connection)
            .await?;
            Ok::<_, Error>(locked.then_some(connection))
        };
        let connection = tokio::select! {
            biased;
            _ = stop.changed() => return,
            result = acquired => result,
        };
        if let Ok(Some(mut connection)) = connection {
            telemetry::push_dispatcher(true);
            let _gauge = HolderGauge;
            if hold(
                &store,
                &config,
                &maintenance,
                &senders,
                &mut connection,
                &mut stop,
            )
            .await
            .is_err()
            {
                tracing::warn!("push dispatcher database operation failed");
            }
            // Closing the dedicated session releases its advisory lock.
            let _ = connection.close().await;
        }
        if stop.borrow().is_some() {
            return;
        }
        tokio::select! { _ = stop.changed() => return, _ = sleep(interval) => {} }
    }
}

struct Loader {
    position: i64,
    boundary: i64,
    bounds: Option<(i64, i64)>,
    after: (i64, Vec<u8>),
    payloads: HashMap<i64, Option<Vec<u8>>>,
}

/// Own all cursor writes on the lock session. Loading and sending run in
/// separate tasks; permit waits and DNS cannot block later window reads.
async fn hold(
    store: &Store,
    config: &Config,
    maintenance: &Notify,
    senders: &Senders,
    connection: &mut PgConnection,
    stop: &mut watch::Receiver<Option<Instant>>,
) -> Result<(), Error> {
    let mut persisted: i64 =
        sqlx::query_scalar!("SELECT sequence_id FROM push_cursor WHERE singleton")
            .fetch_one(&mut *connection)
            .await?;
    let mut work = Work::new(persisted);
    let interval = Duration::from_millis(config.streams.poll_interval_ms);
    let mut tick = Instant::now();
    let mut expiry = Instant::now();
    let mut boundary = persisted;
    let mut exhausted = false;
    let mut loader_state = None;
    let mut loading = JoinSet::new();
    let mut attempts: JoinSet<(Attempt, Outcome)> = JoinSet::new();
    let mut deadline;
    let mut failure = None;
    loop {
        deadline = *stop.borrow_and_update();
        if deadline.is_some() {
            loading.abort_all();
        }
        if let Some(error) = failure.take() {
            telemetry::push_dispatcher(false);
            loading.abort_all();
            // Finish only active attempts after loss. Shutdown may shorten this
            // wait, but no further cursor or recipient write can occur here.
            while !attempts.is_empty() {
                let end = *stop.borrow();
                let remaining = end.map_or(ATTEMPT_TIMEOUT, |end| {
                    end.saturating_duration_since(Instant::now())
                });
                tokio::select! {
                    _ = attempts.join_next() => {},
                    _ = stop.changed() => {},
                    _ = sleep(remaining) => break,
                }
            }
            return Err(error);
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            break;
        }
        while let Some(attempt) = work.next(Instant::now()) {
            let sender = senders.get(attempt.delivery.config.channel);
            attempts.spawn(async move {
                let outcome = match sender {
                    Some(sender) => timeout(ATTEMPT_TIMEOUT, sender.send(&attempt.delivery))
                        .await
                        .unwrap_or(Outcome::Transient { retry_after: None }),
                    None => {
                        tracing::warn!(
                            channel = attempt.delivery.config.channel.label(),
                            "push channel is not configured"
                        );
                        Outcome::Unconfigured
                    }
                };
                (attempt, outcome)
            });
        }
        if deadline.is_some() && work.is_empty() {
            break;
        }
        if Instant::now() >= tick {
            let poll = async {
                // This statement is required even when the low-water mark stalls.
                sqlx::query!("SELECT 1 AS alive")
                    .fetch_one(&mut *connection)
                    .await?;
                persist(connection, &mut persisted, work.low_watermark()).await?;
                // Keep completed progress durable during drain, without loading
                // new windows or starting maintenance work.
                if deadline.is_some() {
                    return Ok(boundary);
                }
                let next_boundary = sqlx::query_scalar!(
                    "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton",
                )
                .fetch_one(&store.read)
                .await?;
                if has_unsettled_envelopes(store, next_boundary).await? {
                    maintenance.notify_one();
                }
                if Instant::now() >= expiry {
                    let removed = sqlx::query!("DELETE FROM push_recipient WHERE renewed_ns < (extract(epoch FROM clock_timestamp()) * 1000000000)::bigint - $1",
                    config.push.recipient_ttl_seconds * xmtp_common::NS_IN_SEC)
                    .execute(&mut *connection).await?.rows_affected();
                    telemetry::push_recipients_removed("expired", removed);
                    expiry = Instant::now() + EXPIRY_INTERVAL;
                }
                Ok::<_, Error>(next_boundary)
            };
            let polled = tokio::select! {
                biased;
                _ = stop.changed() => continue,
                _ = sleep(deadline.map_or(ATTEMPT_TIMEOUT, |end| end.saturating_duration_since(Instant::now()))), if deadline.is_some() => break,
                result = poll => result,
            };
            match polled {
                Ok(next_boundary) => boundary = next_boundary,
                Err(error) => {
                    failure = Some(error);
                    continue;
                }
            }
            tick = Instant::now() + interval;
            exhausted = false;
        }
        if deadline.is_none()
            && stop.borrow().is_none()
            && loading.is_empty()
            && !exhausted
            && work.room_for_page()
        {
            work.reserve_page();
            let mut state = loader_state.take().unwrap_or(Loader {
                position: work.read_position,
                boundary,
                bounds: None,
                after: (work.read_position, Vec::new()),
                payloads: HashMap::new(),
            });
            let store = store.clone();
            loading.spawn(async move {
                let result = window::load(
                    &store,
                    state.position,
                    state.boundary,
                    &state.after,
                    &mut state.payloads,
                )
                .await;
                (state, result)
            });
        }
        let until_tick = tick.saturating_duration_since(Instant::now());
        let delay = work.next_delay(Instant::now()).min(until_tick);
        let delay = deadline.map_or(delay, |end| {
            delay.min(end.saturating_duration_since(Instant::now()))
        });
        tokio::select! {
            biased;
            _ = stop.changed() => {},
            result = attempts.join_next(), if !attempts.is_empty() => {
                let Some(Ok((attempt, outcome))) = result else { failure = Some(Error::Invariant("push sender task failed")); continue };
                let channel = attempt.delivery.config.channel;
                match work.complete(attempt, outcome, config.push.max_attempts) {
                    Completion::Done(outcome) => telemetry::push_delivery(channel, outcome),
                    Completion::Retry => {},
                    Completion::Dead(config) => {
                        telemetry::push_delivery(channel, "dead");
                        match delete_dead(store, &config).await {
                            Ok(true) => {
                                telemetry::push_recipients_removed("dead", 1);
                                work.deleted(&config);
                            },
                            Ok(false) => {},
                            Err(error) => { failure = Some(error); },
                        }
                    },
                }
            },
            result = loading.join_next(), if !loading.is_empty() && deadline.is_none() => {
                let Some(Ok((mut state, result))) = result else { failure = Some(Error::Invariant("push loader task failed")); continue };
                work.release_page();
                let page = match result { Ok(page) => page, Err(error) => { failure = Some(error); continue } };
                for channel in page.suppressed { telemetry::push_delivery(channel, "suppressed"); }
                if let Some((first, last)) = state.bounds.or(page.first.zip(page.last)) {
                    work.add_page(first, last, page.full, page.deliveries);
                    if page.full {
                        // Pruning may remove rows between pages. Preserve the
                        // original window identity and end until paging ends.
                        state.bounds = Some((first, last));
                        state.boundary = last;
                        state.after = page.next;
                        loader_state = Some(state);
                    }
                } else { exhausted = true; }
            },
            _ = sleep(delay) => {},
        }
    }
    loading.abort_all();
    attempts.abort_all();
    let write = persist(connection, &mut persisted, work.low_watermark());
    if let Some(end) = deadline {
        let _ = timeout(end.saturating_duration_since(Instant::now()), write).await;
    } else {
        write.await?;
    }
    Ok(())
}

/// Compare the complete configuration, so a delayed provider response cannot
/// delete a recipient that registered a new secret, key, channel, or target.
// implements: PUSH-234
#[xmtp_common::db_span]
pub(super) async fn delete_dead(store: &Store, config: &DeliveryConfig) -> Result<bool, Error> {
    Ok(sqlx::query!("DELETE FROM push_recipient WHERE recipient_id = $1 AND channel = $2 AND delivery = $3 AND signing_key IS NOT DISTINCT FROM $4 AND secret_hash = $5",
        &config.recipient_id, config.channel as i16, &config.delivery,
        config.signing_key.as_deref(), &config.secret_hash).execute(&store.primary).await?.rows_affected() != 0)
}

/// Move the cursor forward only on the dedicated advisory-lock connection.
#[xmtp_common::db_span]
pub(super) async fn persist(
    connection: &mut PgConnection,
    previous: &mut i64,
    next: i64,
) -> Result<(), Error> {
    if next <= *previous {
        return Ok(());
    }
    let changed = sqlx::query!("UPDATE push_cursor SET sequence_id = $1 WHERE singleton AND sequence_id = $2 AND sequence_id < $1",
        next, *previous).execute(connection).await?.rows_affected();
    if changed == 0 {
        return Err(Error::Invariant("push cursor ownership changed"));
    }
    *previous = next;
    Ok(())
}
