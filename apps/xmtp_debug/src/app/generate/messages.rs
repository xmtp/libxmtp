use crate::DbgClient;
use crate::app::store::Database;
use crate::app::types::{InboxId, Message};
use crate::app::{App, load_all_identities};
use crate::metrics::record_phase_metric;
use crate::{
    app::{
        self,
        store::{GroupStore, IdentityStore, MessageStore, RandomDatabase},
    },
    args,
};
use color_eyre::eyre::WrapErr;
use color_eyre::eyre::{self, Result, eyre};
use indicatif::{ProgressBar, ProgressStyle};
use rand::{RngExt, SeedableRng, prelude::IteratorRandom, rngs::SmallRng, seq::IndexedRandom};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_mls::groups::summary::SyncSummary;

/// Mirror a successfully-sent message to redb's `MessageStore`. Soft
/// errors only — generate is best-effort, so logged-and-skipped rather
/// than aborted. Non-16-byte group_id or non-32-byte message_id would
/// indicate a libxmtp invariant break the validator path already
/// surfaces; we just don't record those rows.
fn record_generated_message(
    store: &MessageStore<'static>,
    group_id: &[u8],
    message_id: &[u8],
    sender_inbox_id: InboxId,
) {
    let Ok(group_id_bytes) = <[u8; 16]>::try_from(group_id) else {
        tracing::warn!(
            target: "generate",
            len = group_id.len(),
            "expected 16-byte group_id; skipping redb message record",
        );
        return;
    };
    let Ok(message_id_bytes) = <[u8; 32]>::try_from(message_id) else {
        tracing::warn!(
            target: "generate",
            len = message_id.len(),
            "expected 32-byte message_id; skipping redb message record",
        );
        return;
    };
    let sent_at_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    let msg = Message::new(
        message_id_bytes,
        group_id_bytes,
        sender_inbox_id,
        sent_at_ns,
    );
    if let Err(e) = store.set(msg) {
        tracing::warn!(target: "generate", error = %e, "redb set failed; skipping message record");
    }
}

mod content_type;

type ConcSemaphore = Arc<tokio::sync::Semaphore>;
type IdentityMap = Arc<HashMap<InboxId, Mutex<crate::DbgClient>>>;

#[derive(thiserror::Error, Debug)]
enum MessageSendError {
    #[error("No group")]
    NoGroup,
    #[error(transparent)]
    Eyre(#[from] eyre::Error),
    #[error(transparent)]
    Client(#[from] xmtp_mls::client::ClientError),
    #[error(transparent)]
    Group(#[from] xmtp_mls::groups::GroupError),
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Sync(#[from] SyncSummary),
    #[error(transparent)]
    Semaphore(#[from] tokio::sync::AcquireError),
}

pub struct GenerateMessages {
    opts: args::MessageGenerateOpts,
    identity_store: IdentityStore<'static>,
    group_store: GroupStore<'static>,
    message_store: MessageStore<'static>,
    identities: IdentityMap,
    semaphore: ConcSemaphore,
}

impl GenerateMessages {
    pub async fn new(opts: args::MessageGenerateOpts, concurrency: usize) -> Result<Self> {
        // Always open write-capable redb so we can mirror sent messages
        // into `MessageStore`. add_member/change_description already
        // required write; default path now does too because we record
        // every successful send.
        let db = App::db()
            .wrap_err("must have exclusive write access to record sent messages in redb")?;
        let identity_store: IdentityStore<'static> = db.clone().into();
        let group_store: GroupStore<'static> = db.clone().into();
        let message_store: MessageStore<'static> = db.into();
        let identities = load_all_identities(&identity_store).await?;
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        Ok(Self {
            opts,
            identity_store,
            group_store,
            message_store,
            identities,
            semaphore,
        })
    }

    /// Returns Vec of send_message latencies (only the actual send, not sync overhead)
    pub async fn run(self, n: usize) -> Result<Vec<Duration>> {
        info!(fdlimit = app::get_fdlimit(), "generating messages");
        let args::MessageGenerateOpts {
            r#loop,
            interval,
            change_description,
            add_and_change_description,
            add_up_to,
            ..
        } = self.opts;

        let loop_pause_secs: Option<u64> = std::env::var("XDBG_LOOP_PAUSE")
            .ok()
            .and_then(|v| v.parse().ok());

        let mut all_latencies = self.send_many_messages(n).await?;

        if r#loop {
            loop {
                if let Some(secs) = loop_pause_secs {
                    tracing::debug!(secs, "sleeping XDBG_LOOP_PAUSE after messages");
                    tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                }
                tokio::time::sleep(*interval).await;
                let semaphore = self.semaphore.clone();
                let group_store = self.group_store.clone();
                let identities = self.identities.clone();
                let (latencies, _, _) = tokio::try_join!(
                    self.send_many_messages(n),
                    flatten(tokio::spawn(Self::add_member(
                        add_and_change_description,
                        add_up_to,
                        semaphore.clone(),
                        group_store.clone(),
                        identities.clone()
                    ))),
                    flatten(tokio::spawn(Self::change_group_description(
                        change_description || add_and_change_description,
                        semaphore.clone(),
                        group_store.clone(),
                        identities.clone()
                    ))),
                )?;
                all_latencies.extend(latencies);
            }
        }
        Ok(all_latencies)
    }

    async fn add_member(
        run: bool,
        add_up_to: u32,
        semaphore: ConcSemaphore,
        group_store: GroupStore<'static>,
        identities: IdentityMap,
    ) -> Result<()> {
        if !run {
            return Ok(());
        }
        info!(time = ?std::time::Instant::now(), "adding new member");
        let rng = &mut SmallRng::from_rng(&mut rand::rng());
        let group = group_store
            .random(rng)?
            .ok_or(eyre!("no group in local store"))?;
        if group.members.len() >= add_up_to.try_into()? {
            // added up to required amount
            return Ok(());
        }
        let _permit = semaphore.acquire().await?;
        let members: HashSet<&[u8; 32]> = HashSet::from_iter(group.members.iter());
        let not_in_group = identities
            .keys()
            .filter(|id| !members.contains(id))
            .choose(rng)
            .ok_or(eyre!("no identity exists that is not already in group"))?;
        let owner = identities
            .get(&group.created_by)
            .ok_or(eyre!("group has no owner"))?;
        let owner = owner.lock().await;
        let owner_group = owner.group(&group.id()).wrap_err(format!(
            "owner {} of group {} failed to look up in sqlite db",
            hex::encode(group.created_by),
            group.id()
        ))?;
        owner_group
            .add_members(&[hex::encode(not_in_group)])
            .await
            .inspect_err(|e| error!(%group, "{}", e))?;
        // make sure to update the group metadata
        let mut new_group = group.clone();
        new_group.members.push(*not_in_group);
        new_group.member_size += 1;
        group_store
            .set(new_group)
            .wrap_err("failed to update group with new member in redb index")?;
        Ok(())
    }

    async fn change_group_description(
        run: bool,
        semaphore: ConcSemaphore,
        group_store: GroupStore<'static>,
        identities: IdentityMap,
    ) -> Result<()> {
        if !run {
            return Ok(());
        }
        let _permit = semaphore.acquire().await?;
        let rng = &mut SmallRng::from_rng(&mut rand::rng());
        let clients = identities.clone();
        let group = group_store
            .random(rng)?
            .ok_or(eyre!("no group in local store"))?;
        if let Some(inbox_id) = group.members.choose(rng) {
            let client = clients
                .get(inbox_id.as_slice())
                .ok_or(eyre!("client does not exist"))?;
            let client = client.lock().await;
            client.sync_welcomes().await?;
            let mls_group = client.group(&group.id())?;
            mls_group.sync_with_conn().await?;
            mls_group.maybe_update_installations(None).await?;
            let words = rng.random_range(0..10);
            let words = lipsum::lipsum_words(words as usize);
            info!(time = ?std::time::Instant::now(), new_description=words, "updating group description");
            mls_group
                .update_group_description(words)
                .await
                .inspect_err(|e| error!(%group, "{}", e))?;
            Ok(())
        } else {
            Err(MessageSendError::NoGroup.into())
        }
    }

    /// Returns a Vec of send_message latencies (only the actual send, not sync overhead)
    async fn send_many_messages(&self, n: usize) -> Result<Vec<Duration>> {
        let Self { opts, .. } = self;

        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());

        let semaphore = self.semaphore.clone();
        let clients = self.identities.clone();
        let mut set: tokio::task::JoinSet<Result<Duration, eyre::Error>> =
            tokio::task::JoinSet::new();
        let stores = (
            self.identity_store.clone(),
            self.group_store.clone(),
            self.message_store.clone(),
        );
        for _ in 0..n {
            let bar_pointer = bar.clone();
            let opts = *opts;
            let (_, group, messages) = stores.clone();
            let semaphore = semaphore.clone();
            let cs = clients.clone();
            set.spawn(async move {
                let _permit = semaphore.acquire().await?;
                let latency = Self::send_message(&group, &messages, cs, opts)
                    .await
                    .inspect_err(|e| error!("{}", e))?;
                bar_pointer.inc(1);
                Ok(latency)
            });
        }

        let res = set.join_all().await;

        bar.finish();
        bar.reset();

        let errors: Vec<_> = res
            .iter()
            .filter(|r| r.is_err())
            .map(|r| r.as_ref().unwrap_err())
            .collect();

        if !errors.is_empty() {
            info!(errors = ?errors, "errors");
            if crate::fail_fast() {
                let first = errors[0].to_string();
                return Err(eyre!(
                    "{} of {} send_message tasks failed (--fail-fast): {}",
                    errors.len(),
                    res.len(),
                    first
                ));
            }
        }

        let latencies: Vec<Duration> = res.into_iter().filter_map(|r| r.ok()).collect();
        Ok(latencies)
    }

    /// Returns the duration of just the send_message() call (excluding sync overhead)
    async fn send_message(
        group_store: &GroupStore<'static>,
        message_store: &MessageStore<'static>,
        clients: Arc<HashMap<InboxId, Mutex<DbgClient>>>,
        opts: args::MessageGenerateOpts,
    ) -> Result<Duration, MessageSendError> {
        let args::MessageGenerateOpts {
            ref max_message_size,
            ..
        } = opts;

        let rng = &mut SmallRng::from_rng(&mut rand::rng());
        let stored_group = group_store
            .random(rng)?
            .ok_or(eyre!("no group in local store"))?;
        info!(time = ?Instant::now(), group = %stored_group.id(), "sending message");
        let Some(sender_inbox_id) = stored_group.members.choose(rng).copied() else {
            return Err(MessageSendError::NoGroup);
        };
        let client = clients
            .get(sender_inbox_id.as_slice())
            .ok_or(eyre!("client does not exist"))?;
        let client = client.lock().await;
        client.sync_welcomes().await?;
        let mls_group = client.group(&stored_group.id())?;
        mls_group.sync_with_conn().await?;
        mls_group.maybe_update_installations(None).await?;
        let words = rng.random_range(0..*max_message_size);
        let words = lipsum::lipsum_words(words as usize);
        let message = content_type::new_message(words);

        // Time ONLY the send_message() call
        let start = Instant::now();
        let message_id = mls_group
            .send_message(
                &message,
                SendMessageOptsBuilder::default()
                    .should_push(true)
                    .build()
                    .unwrap(),
            )
            .await?;
        let send_latency = start.elapsed();
        let send_secs = send_latency.as_secs_f64();

        record_phase_metric("send_message", send_secs, "send_message", "xdbg_debug").await;

        // Mirror to redb so healthcheck's `NoMissingMessages` validator
        // and any cross-tool inspection can find this message later.
        record_generated_message(
            message_store,
            &*stored_group.id(),
            &message_id,
            sender_inbox_id,
        );

        Ok(send_latency)
    }
}

async fn flatten<T>(handle: JoinHandle<Result<T>>) -> Result<T> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(eyre!("spawned task failed {err}")),
    }
}
