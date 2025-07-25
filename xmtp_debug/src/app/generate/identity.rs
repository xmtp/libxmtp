use std::{collections::HashSet, sync::Arc};

use crate::app::store::{Database, IdentityStore};
use crate::app::{self, types::Identity};
use crate::args;

use color_eyre::eyre::{self, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};

/// Identity Generation
pub struct GenerateIdentity {
    identity_store: IdentityStore<'static>,
    network: args::BackendOpts,
}

impl GenerateIdentity {
    pub fn new(identity_store: IdentityStore<'static>, network: args::BackendOpts) -> Self {
        Self {
            identity_store,
            network,
        }
    }

    #[allow(unused)]
    pub fn load_identities(
        &self,
    ) -> Result<Option<impl Iterator<Item = Result<Identity>> + use<'_>>> {
        Ok(self
            .identity_store
            .load(&self.network)?
            .map(|i| i.map(|i| Ok(i.value()))))
    }

    /// Create identities if they don't already exist.
    /// creates specified `identities` on the
    /// gRPC local docker or development node and saves them to a file.
    /// `identities.generated`/`dev-identities.generated`. Uses this file for subsequent runs if
    /// node still has those identities.
    #[allow(unused)]
    pub async fn create_identities_if_dont_exist(
        &self,
        n: usize,
        client: &crate::DbgClient,
    ) -> Result<Vec<Identity>> {
        let connection = client.context.store().db();
        if let Some(mut identities) = self.load_identities()? {
            let first = identities.next().ok_or(eyre::eyre!("Does not exist"))??;

            let state = client
                .identity_updates()
                .get_latest_association_state(&connection, &hex::encode(first.inbox_id))
                .await?;
            info!("Found generated identities, checking for registration on backend...",);
            // we assume that if the first identity is registered, they all are
            if !state.members().is_empty() {
                return identities.collect::<Result<Vec<Identity>, _>>();
            } else {
                warn!(
                    "No identities found for network {}, clearing orphans and re-instantiating",
                    &url::Url::from(self.network.clone())
                );
                self.identity_store.clear_network(&self.network)?;
            }
        }
        info!("Could not find identities to load, creating new identities");
        let identities = self.create_identities(n, 10).await?;
        self.identity_store
            .set_all(identities.as_slice(), &self.network)?;
        Ok(identities)
    }

    pub async fn create_identities(&self, n: usize, concurrency: usize) -> Result<Vec<Identity>> {
        let mut identities: Vec<Identity> = Vec::with_capacity(n);

        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());
        let mut set: tokio::task::JoinSet<Result<_, eyre::Error>> = tokio::task::JoinSet::new();

        let network = &self.network;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        for _ in 0..n {
            let bar_pointer = bar.clone();
            let network = network.clone();
            let semaphore = semaphore.clone();
            set.spawn(async move {
                let _permit = semaphore.acquire().await?;
                let wallet = crate::app::generate_wallet();
                // TODO: maybe create all new clients in a temp directory
                // then copy + store at the same time
                // in case CLI is exited before finishing
                let user = app::new_registered_client(network, Some(&wallet)).await?;
                bar_pointer.inc(1);
                Identity::from_libxmtp(user.identity(), wallet)
            });

            if set.len() == app::get_fdlimit() {
                if let Some(identity) = set.join_next().await {
                    match identity {
                        Ok(identity) => {
                            identities.push(identity?);
                        }
                        Err(e) => {
                            error!("{}", e.to_string());
                        }
                    }
                }
            }
        }

        while let Some(identity) = set.join_next().await {
            match identity {
                Ok(identity) => {
                    identities.push(identity?);
                }
                Err(e) => {
                    error!("{}", e.to_string());
                }
            }
        }

        self.identity_store
            .set_all(identities.as_slice(), &self.network)?;

        bar.finish();
        bar.reset();
        let mut set: tokio::task::JoinSet<Result<_, eyre::Error>> = tokio::task::JoinSet::new();
        // ensure all the identities are registered
        let tmp = Arc::new(app::temp_client(network, None).await?);
        let conn = Arc::new(tmp.context.store().db());
        let bar_ref = bar.clone();
        let future = |inbox_id: [u8; 32]| async move {
            let id = hex::encode(inbox_id);
            trace!(inbox_id = id, "getting association state");
            let state = tmp
                .identity_updates()
                .get_latest_association_state(&conn, &id)
                .await?;
            bar_ref.inc(1);
            Ok(state)
        };

        identities.as_slice().iter().for_each(|i| {
            set.spawn(future.clone()(i.inbox_id));
        });
        bar.finish_and_clear();
        let states = set.join_all().await;
        info!(
            total_states = states.len(),
            "ensuring identities registered & latest association state loaded..."
        );
        let errs = states
            .into_iter()
            .filter_map(|s| s.err())
            .map(|e| e.to_string())
            .collect::<Vec<String>>();
        let unique: HashSet<String> = HashSet::from_iter(errs.clone());
        if !unique.is_empty() {
            tracing::error!("{} errors during identity generation", errs.len());
            tracing::error!("{} unique errors during identity generation", unique.len());
            for err in unique.into_iter() {
                error!(err);
            }
            bail!("Error generation failed");
        }
        Ok(identities)
    }
}
