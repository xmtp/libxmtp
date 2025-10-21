//! Get information about xdbg storage

use crate::app::App;
use crate::app::store::{Database, MetadataStore};
use crate::args;
use valuable::Valuable;

use color_eyre::eyre::Result;

pub struct Info {
    opts: args::InfoOpts,
    metadata_store: MetadataStore<'static>,
    network: args::BackendOpts,
}

impl Info {
    pub fn new(opts: args::InfoOpts, network: args::BackendOpts) -> Result<Self> {
        let db = App::readonly_db()?;
        Ok(Self {
            opts,
            network,
            metadata_store: db.into(),
        })
    }

    pub async fn run(self) -> Result<()> {
        let Info { opts, .. } = &self;
        // if we didn't give any options
        if (opts.app as u8) + (opts.random as u8) == 0 {
            self.app()?;
        }

        if opts.app {
            self.app()?;
        }

        // if opts.random {}
        Ok(())
    }

    fn app(&self) -> Result<()> {
        let metadata = self
            .metadata_store
            .get((&self.network).into())
            .ok()
            .flatten()
            .unwrap_or(Default::default());

        let sqlite_stores = crate::app::App::db_directory(&self.network)?;
        let db_dir_size = fs_extra::dir::get_size(&sqlite_stores)? / 1_000 / 1_000;
        info!(
            metadata.identities,
            metadata.groups,
            // metadata.messages,
            project = crate::app::App::data_directory()?.as_value(),
            sqlite = sqlite_stores.as_value(),
            sqlite_size = %format!("{db_dir_size}MB"),
            app_db = crate::app::App::redb()?.as_value(),
            "App Information"
        );
        Ok(())
    }
}
