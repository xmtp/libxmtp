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
            args::Query::Identity(opts) => self.identity(&opts.inbox_id).await,
            args::Query::FetchKeyPackages(_) => self.fetch_key_packages().await,
            args::Query::BatchQueryCommitLog(_) => self.batch_query_commit_log().await,
        }
    }

    pub async fn identity(&self, inbox_id: &args::InboxId) -> Result<()> {
        tracing::info!("Fetching identity for inbox: {}", inbox_id);
        Ok(())
    }

    pub async fn fetch_key_packages(self) -> Result<()> {
        tracing::info!("Fetching key packages");
        Ok(())
    }

    pub async fn batch_query_commit_log(self) -> Result<()> {
        tracing::info!("Batch querying commit log");
        Ok(())
    }
}
