use crate::args;
use color_eyre::eyre::Result;
use std::sync::Arc;

pub struct Query {
    opts: args::Query,
    #[allow(unused)]
    network: args::BackendOpts,
    #[allow(unused)]
    store: Arc<redb::Database>,
}

impl Query {
    pub fn new(opts: args::Query, network: args::BackendOpts, store: Arc<redb::Database>) -> Self {
        Self {
            opts,
            network,
            store,
        }
    }

    pub async fn run(self) -> Result<()> {
        match &self.opts {
            args::Query::Identity(opts) => self.identity(opts).await,
            args::Query::FetchKeyPackages(opts) => self.fetch_key_packages(opts).await,
            args::Query::BatchQueryCommitLog(opts) => self.batch_query_commit_log(opts).await,
        }
    }

    pub async fn identity(&self, opts: &args::Identity) -> Result<()> {
        tracing::info!("Fetching identity for inbox: {}", opts.inbox_id);
        Ok(())
    }

    pub async fn fetch_key_packages(&self, opts: &args::FetchKeyPackages) -> Result<()> {
        tracing::info!("Fetching key packages");
        let _ = opts;
        Ok(())
    }

    pub async fn batch_query_commit_log(&self, opts: &args::BatchQueryCommitLog) -> Result<()> {
        tracing::info!("Batch querying commit log");
        let _ = opts;
        Ok(())
    }
}
