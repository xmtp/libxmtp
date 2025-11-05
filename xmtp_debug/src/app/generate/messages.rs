use crate::DbgClient;
use crate::app::types::InboxId;
use crate::app::{App, load_all_identities};
use crate::{
    app::{
        self,
        store::{GroupStore, IdentityStore, RandomDatabase},
    },
    args,
};
use color_eyre::eyre::{self, Result, eyre};
use indicatif::{ProgressBar, ProgressStyle};
use rand::{Rng, SeedableRng, rngs::SmallRng, seq::SliceRandom};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_mls::groups::summary::SyncSummary;

mod content_type;

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
}

pub struct GenerateMessages {
    network: args::BackendOpts,
    opts: args::MessageGenerateOpts,
    identity_store: IdentityStore<'static>,
    group_store: GroupStore<'static>,
}

impl GenerateMessages {
    pub fn new(network: args::BackendOpts, opts: args::MessageGenerateOpts) -> Result<Self> {
        let (identity_store, group_store) = {
            let db = App::readonly_db()?;
            (db.clone().into(), db.into())
        };
        Ok(Self {
            network,
            opts,
            identity_store,
            group_store,
        })
    }

    pub async fn run(self, n: usize, concurrency: usize) -> Result<()> {
        info!(fdlimit = app::get_fdlimit(), "generating messages");
        let args::MessageGenerateOpts {
            r#loop, interval, ..
        } = self.opts;

        self.send_many_messages(n, concurrency).await?;

        if r#loop {
            loop {
                info!(time = ?std::time::Instant::now(), amount = n, "sending messages");
                tokio::time::sleep(*interval).await;
                self.send_many_messages(n, concurrency).await?;
            }
        }
        Ok(())
    }

    async fn send_many_messages(&self, n: usize, concurrency: usize) -> Result<usize> {
        let Self { network, opts, .. } = self;

        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        let clients = load_all_identities(&self.identity_store, network).await?;
        let mut set: tokio::task::JoinSet<Result<(), eyre::Error>> = tokio::task::JoinSet::new();
        let stores = (self.identity_store.clone(), self.group_store.clone());
        for _ in 0..n {
            let bar_pointer = bar.clone();
            let n = network.clone();
            let opts = opts.clone();
            let (_, group) = stores.clone();
            let semaphore = semaphore.clone();
            let cs = clients.clone();
            set.spawn(async move {
                let _permit = semaphore.acquire().await?;
                Self::send_message(&group, cs, n, opts).await?;
                bar_pointer.inc(1);
                Ok(())
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
        }

        let msgs_sent = res
            .into_iter()
            .filter(|r| r.is_ok())
            .collect::<Vec<Result<_, _>>>();
        Ok(msgs_sent.len())
    }

    async fn send_message(
        group_store: &GroupStore<'static>,
        clients: Arc<HashMap<InboxId, Mutex<DbgClient>>>,
        network: args::BackendOpts,
        opts: args::MessageGenerateOpts,
    ) -> Result<(), MessageSendError> {
        let args::MessageGenerateOpts {
            ref max_message_size,
            ..
        } = opts;

        let rng = &mut SmallRng::from_entropy();
        let group = group_store
            .random(&network, rng)?
            .ok_or(eyre!("no group in local store"))?;
        if let Some(inbox_id) = group.members.choose(rng) {
            let client = clients
                .get(inbox_id.as_slice())
                .ok_or(eyre!("client does not exist"))?;
            let client = client.lock().await;
            client.sync_welcomes().await?;
            let group = client.group(&group.id.into())?;
            group.sync_with_conn().await?;
            group.maybe_update_installations(None).await?;
            let words = rng.gen_range(0..*max_message_size);
            let words = lipsum::lipsum_words_with_rng(&mut *rng, words as usize);
            let message = content_type::new_message(words);
            group
                .send_message(
                    &message,
                    SendMessageOptsBuilder::default()
                        .should_push(true)
                        .build()
                        .unwrap(),
                )
                .await?;
            Ok(())
        } else {
            Err(MessageSendError::NoGroup)
        }
    }
}
