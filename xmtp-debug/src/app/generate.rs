use crate::args;

use super::types::Identity;
use crate::app::store::IdentityStore;

use color_eyre::eyre::{self, Result};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Debug)]
pub struct Generate {
    identity_store: IdentityStore,
    opts: args::Generate,
    network: args::BackendOpts,
}

impl Generate {
    pub fn new(
        opts: args::Generate,
        network: args::BackendOpts,
        identity_store: IdentityStore,
    ) -> Self {
        Self {
            opts,
            network,
            identity_store,
        }
    }

    pub async fn run(self) -> Result<()> {
        use args::EntityKind::*;
        let Generate {
            identity_store,
            opts,
            network,
        } = self;
        let args::Generate { entity, amount } = opts;

        match entity {
            Group => {
                todo!()
            }
            Message => {
                todo!()
            }
            Identity => {
                let temp = crate::app::temp_client(&network, None).await?;
                GenerateIdentity::new(identity_store, network)
                    .create_identities_if_dont_exist(amount, &temp)
                    .await?;
                Ok(())
            }
        }
    }
}

/// Identity Generation
pub struct GenerateIdentity {
    identity_store: IdentityStore,
    network: args::BackendOpts,
}

impl GenerateIdentity {
    pub fn new(identity_store: IdentityStore, network: args::BackendOpts) -> Self {
        Self {
            identity_store,
            network,
        }
    }

    pub fn load_identities(&self) -> Result<impl Iterator<Item = Result<Identity>>> {
        self.identity_store.identities(&self.network)
    }

    /// Create identities if they don't already exist.
    /// creates specified `identities` on the
    /// gRPC local docker or development node and saves them to a file.
    /// `identities.generated`/`dev-identities.generated`. Uses this file for subsequent runs if
    /// node still has those identities.
    pub async fn create_identities_if_dont_exist(
        &self,
        n: usize,
        client: &crate::DbgClient,
    ) -> Result<Vec<Identity>> {
        let connection = client.store().conn()?;
        let mut identities = self.load_identities()?;
        let first = identities.next().ok_or(eyre::eyre!("Does not exist"))??;
        let state = client
            .get_latest_association_state(&connection, hex::encode(&first.inbox_id))
            .await?;
        tracing::info!(
            "Found {:?} generated identities, checking for existence on backend...",
            identities.size_hint()
        );
        if state.members().len() > 0 {
            return Ok(identities.collect::<Result<Vec<Identity>, _>>()?);
        }
        tracing::info!(
            "Could not find any identitites to load, creating new identitites \n
            Beware, this fills $TMPDIR with ~10GBs of identities"
        );

        let identities = self.create_identities(n).await?;
        // println!("Writing {identities} identities... (this will take a while...)");
        // let addresses = write_identities(identities, is_dev_network).await;
        // println!("Wrote {identities} to {}", file_path(is_dev_network));
        //addresses
        Ok(identities)
    }

    async fn create_identities(&self, n: usize) -> Result<Vec<Identity>> {
        let mut identities: Vec<Identity> = Vec::with_capacity(n);

        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());
        let mut set: tokio::task::JoinSet<Result<_, eyre::Error>> = tokio::task::JoinSet::new();
        let mut handles = vec![];

        let network = &self.network;
        for _ in 0..n {
            let bar_pointer = bar.clone();
            let network = network.clone();
            handles.push(set.spawn(async move {
                let wallet = crate::app::generate_wallet();
                let user = super::client(network, Some(&wallet)).await?;
                bar_pointer.inc(1);
                Identity::from_libxmtp(user.identity(), wallet)
            }));

            // going above 128 we hit "unable to open database errors"
            // This may be related to open file limits
            if set.len() == 128 {
                if let Some(Ok(identity)) = set.join_next().await {
                    identities.push(identity?);
                }
            }
        }

        while let Some(Ok(identity)) = set.join_next().await {
            identities.push(identity?);
        }
        Ok(identities)
    }
}
