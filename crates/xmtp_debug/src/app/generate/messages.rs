use crate::DbgClient;
use crate::app::store::Database;
use crate::app::types::InboxId;
use crate::app::{App, load_all_identities};
use crate::args::BackendOpts;
use crate::{
    app::{
        self,
        store::{GroupStore, IdentityStore, RandomDatabase},
    },
    args,
};
use alloy::primitives::map::HashSet;
use color_eyre::eyre::WrapErr;
use color_eyre::eyre::{self, Result, eyre};
use indicatif::{ProgressBar, ProgressStyle};
use rand::{Rng, SeedableRng, prelude::IteratorRandom, rngs::SmallRng, seq::SliceRandom};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_mls::groups::summary::SyncSummary;

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
    network: args::BackendOpts,
    opts: args::MessageGenerateOpts,
    identity_store: IdentityStore<'static>,
    group_store: GroupStore<'static>,
    identities: IdentityMap,
    semaphore: ConcSemaphore,
}

impl GenerateMessages {
    pub fn new(
        network: args::BackendOpts,
        opts: args::MessageGenerateOpts,
        concurrency: usize,
    ) -> Result<Self> {
        let (identity_store, group_store) = {
            if opts.add_and_change_description || opts.change_description {
                let db = App::db().wrap_err(
                    "must have exclusive write access for adding members or changing description",
                )?;
                (db.clone().into(), db.into())
            } else {
                let db = App::readonly_db()?;
                (db.clone().into(), db.into())
            }
        };
        let identities = load_all_identities(&identity_store, &network)?;
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        Ok(Self {
            network,
            opts,
            identity_store,
            group_store,
            identities,
            semaphore,
        })
    }

    pub async fn run(self, n: usize) -> Result<()> {
        info!(fdlimit = app::get_fdlimit(), "generating messages");
        let args::MessageGenerateOpts {
            r#loop,
            interval,
            change_description,
            add_and_change_description,
            add_up_to,
            ..
        } = self.opts;

        self.send_many_messages(n).await?;

        if r#loop {
            loop {
                tokio::time::sleep(*interval).await;
                let semaphore = self.semaphore.clone();
                let group_store = self.group_store.clone();
                let network = self.network.clone();
                let identities = self.identities.clone();
                tokio::try_join!(
                    self.send_many_messages(n),
                    flatten(tokio::spawn(Self::add_member(
                        add_and_change_description,
                        add_up_to,
                        semaphore.clone(),
                        network.clone(),
                        group_store.clone(),
                        identities.clone()
                    ))),
                    flatten(tokio::spawn(Self::change_group_description(
                        change_description || add_and_change_description,
                        semaphore.clone(),
                        network.clone(),
                        group_store.clone(),
                        identities.clone()
                    ))),
                )?;
            }
        }
        Ok(())
    }

    async fn add_member(
        run: bool,
        add_up_to: u32,
        semaphore: ConcSemaphore,
        network: BackendOpts,
        group_store: GroupStore<'static>,
        identities: IdentityMap,
    ) -> Result<()> {
        if !run {
            return Ok(());
        }
        info!(time = ?std::time::Instant::now(), "adding new member");
        let rng = &mut SmallRng::from_entropy();
        let group = group_store
            .random(&network, rng)?
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
        let owner_group = owner.group(&group.id.to_vec()).wrap_err(format!(
            "owner {} of group {} failed to look up in sqlite db",
            hex::encode(group.created_by),
            hex::encode(group.id)
        ))?;
        owner_group
            .add_members(&[hex::encode(not_in_group)])
            .await?;
        // make sure to update the group metadata
        let mut new_group = group.clone();
        new_group.members.push(*not_in_group);
        new_group.member_size += 1;
        group_store
            .set(new_group, network)
            .wrap_err("failed to update group with new member in redb index")?;
        Ok(())
    }

    async fn change_group_description(
        run: bool,
        semaphore: ConcSemaphore,
        network: BackendOpts,
        group_store: GroupStore<'static>,
        identities: IdentityMap,
    ) -> Result<()> {
        if !run {
            return Ok(());
        }
        let _permit = semaphore.acquire().await?;
        let rng = &mut SmallRng::from_entropy();
        let clients = identities.clone();
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
            let words = rng.gen_range(0..10);
            let words = lipsum::lipsum_words_with_rng(&mut *rng, words as usize);
            info!(time = ?std::time::Instant::now(), new_description=words, "updating group description");
            group.update_group_description(words).await?;
            Ok(())
        } else {
            Err(MessageSendError::NoGroup.into())
        }
    }

    async fn send_many_messages(&self, n: usize) -> Result<usize> {
        let Self { network, opts, .. } = self;

        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());

        let semaphore = self.semaphore.clone();
        let clients = self.identities.clone();
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
        info!(time = ?std::time::Instant::now(), group = hex::encode(group.id), "sending message");
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

async fn flatten<T>(handle: JoinHandle<Result<T>>) -> Result<T> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(eyre!("spawned task failed {err}")),
    }
}
