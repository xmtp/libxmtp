use crate::{
    app::{
        self,
        store::{Database, GroupStore, IdentityStore, MetadataStore, RandomDatabase},
    },
    args,
};
use color_eyre::eyre::{self, Result, eyre};
use rand::{Rng, SeedableRng, rngs::SmallRng, seq::SliceRandom};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Mutex as TokioMutex;
use xmtp_mls::groups::summary::SyncSummary;

mod content_type;

type IdentityLockMap = Arc<Mutex<HashMap<[u8; 32], Arc<TokioMutex<()>>>>>;

static IDENTITY_LOCKS: OnceLock<IdentityLockMap> = OnceLock::new();

fn get_identity_lock(inbox_id: &[u8; 32]) -> Result<Arc<TokioMutex<()>>, eyre::Error> {
    let locks = IDENTITY_LOCKS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
    let mut map = locks
        .lock()
        .map_err(|e| eyre!("Failed to lock IDENTITY_LOCKS: {}", e))?;
    Ok(map
        .entry(*inbox_id)
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone())
}

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
    db: Arc<redb::Database>,
    network: args::BackendOpts,
    opts: args::MessageGenerateOpts,
}

impl GenerateMessages {
    pub fn new(
        db: Arc<redb::Database>,
        network: args::BackendOpts,
        opts: args::MessageGenerateOpts,
    ) -> Self {
        Self { db, network, opts }
    }

    pub async fn run(self, n: usize) -> Result<()> {
        info!(fdlimit = app::get_fdlimit(), "generating messages");
        let args::MessageGenerateOpts {
            r#loop, interval, ..
        } = self.opts;

        self.send_many_messages(self.db.clone(), n).await?;

        if r#loop {
            loop {
                info!(time = ?std::time::Instant::now(), amount = n, "sending messages");
                tokio::time::sleep(*interval).await;
                self.send_many_messages(self.db.clone(), n).await?;
            }
        }
        Ok(())
    }

    async fn send_many_messages(&self, db: Arc<redb::Database>, n: usize) -> Result<usize> {
        let Self { network, opts, .. } = self;

        let mut set: tokio::task::JoinSet<Result<(), eyre::Error>> = tokio::task::JoinSet::new();
        for _ in 0..n {
            let d = db.clone();
            let n = network.clone();
            let opts = opts.clone();
            set.spawn(async move {
                Self::send_message(&d.clone().into(), &d.clone().into(), n, opts).await?;
                Ok(())
            });
        }

        let res = set.join_all().await;
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
        let key = crate::meta_key!(network);
        let meta_db: MetadataStore = db.into();
        meta_db.modify(key, |meta| {
            meta.messages += msgs_sent.len() as u32;
        })?;
        Ok(msgs_sent.len())
    }

    async fn send_message(
        group_store: &GroupStore<'static>,
        identity_store: &IdentityStore<'static>,
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
            let key = (u64::from(&network), *inbox_id);

            // each identity can only be used by one worker thread
            let identity_lock = get_identity_lock(inbox_id)?;
            let _lock_guard = identity_lock.lock().await;

            let identity = identity_store.get(key.into())?.ok_or(eyre!(
                "No identity with inbox id [{}] in local store",
                hex::encode(inbox_id)
            ))?;
            let client = app::client_from_identity(&identity, &network).await?;
            client.sync_welcomes().await?;
            let group = client.group(&group.id.into())?;
            group.maybe_update_installations(None).await?;
            group.sync_with_conn().await?;
            let words = rng.gen_range(0..*max_message_size);
            let words = lipsum::lipsum_words_with_rng(&mut *rng, words as usize);
            let message = content_type::new_message(words);
            group.send_message(&message).await?;
            Ok(())
        } else {
            Err(MessageSendError::NoGroup)
        }
    }
}
