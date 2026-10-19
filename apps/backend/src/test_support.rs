use crate::{
    Backend, api,
    config::{Config, ValidatedConfig},
    server,
};
pub mod auth;
mod database;
#[cfg(test)]
pub(crate) mod metrics;
#[cfg(test)]
pub(crate) mod push_provider;
pub use database::TestDatabase;
// Only `apps/backend`'s own tests use these. Keep them out of the
// `test-utils` surface that `xmtp_mls` consumes.
#[cfg(test)]
pub(crate) mod grpc_web;
#[cfg(test)]
pub(crate) mod handles;
#[cfg(test)]
pub(crate) mod locks;
#[cfg(test)]
pub(crate) mod native;
#[cfg(test)]
pub(crate) mod replica;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tonic::transport::{Channel, Endpoint};
use xmtp_id::scw_verifier::{
    CachedSmartContractSignatureVerifier, ChainBlocks, SmartContractSignatureVerifier,
};

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Every backend needs an identifier. A fixture that does not care about the
/// value gets this one, so no test has to repeat it.
pub const DEFAULT_TEST_IDENTIFIER: &str = "org.xmtp.test";

/// The file layer of a configuration that names `database_url` and the
/// default test identifier, and sets nothing else.
pub fn config_for(database_url: &str) -> Config {
    Config::from_toml_str(&format!(
        "[database]\nurl = {database_url:?}\n[server]\nidentifier = {DEFAULT_TEST_IDENTIFIER:?}"
    ))
    .expect("the test configuration parses")
}

pub struct TestServer {
    running: RunningServer,
    database: TestDatabase,
}
impl std::ops::Deref for TestServer {
    type Target = RunningServer;
    fn deref(&self) -> &RunningServer {
        &self.running
    }
}
impl std::ops::DerefMut for TestServer {
    fn deref_mut(&mut self) -> &mut RunningServer {
        &mut self.running
    }
}

pub struct RunningServer {
    pub backend: Backend,
    pub channel: Channel,
    pub url: String,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<(), server::ServeError>>>,
}

impl TestServer {
    /// Apply `change` to the file layer of a configuration for a new test
    /// database, validate it, and start a server.
    pub async fn new(change: impl FnOnce(&mut Config)) -> TestResult<Self> {
        Self::start(change, None).await
    }

    /// Apply partial TOML over defaults and resolve environment references.
    /// The fixture owns the database URLs, so the TOML must not set them.
    /// Validate before creating a database. Bind a loopback port chosen by the OS.
    ///
    /// `server.listen` is accepted and validated but has no effect: the fixture
    /// always binds a loopback port chosen by the OS.
    pub async fn from_toml(toml: &str) -> TestResult<Self> {
        let mut value: toml::Table =
            toml::from_str(toml).map_err(|_| crate::config::ConfigError::Parse)?;
        let database = value
            .entry("database")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        let database = database.as_table_mut().ok_or("database must be a table")?;
        // Reject rather than overwrite. A silently dropped URL would give the
        // caller a backend that does not match the configuration it supplied.
        for field in ["url", "replica_url"] {
            if database.contains_key(field) {
                return Err(format!("database.{field} is owned by the fixture").into());
            }
        }
        database.insert("url".into(), "postgres://localhost/ephemeral".into());
        let server = value
            .entry("server")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        let server = server.as_table_mut().ok_or("server must be a table")?;
        server
            .entry("identifier")
            .or_insert_with(|| DEFAULT_TEST_IDENTIFIER.into());
        // Refuse an invalid file before creating a database. The real URL is
        // set on the file layer, and `start` validates it again.
        let config = Config::from_toml_str(&toml::to_string(&value)?)?
            .validate()?
            .into_config();
        Self::start(
            move |defaults| {
                let url = std::mem::take(&mut defaults.database.url);
                *defaults = config;
                defaults.database.url = url;
            },
            None,
        )
        .await
    }

    /// Replace the configured chains with a controlled verifier and chain.
    pub async fn with_verifier(
        change: impl FnOnce(&mut Config),
        verifier: impl SmartContractSignatureVerifier + 'static,
        chain: impl ChainBlocks + 'static,
    ) -> TestResult<Self> {
        Self::start(
            change,
            Some((Box::new(verifier), std::sync::Arc::new(chain))),
        )
        .await
    }

    async fn start(
        change: impl FnOnce(&mut Config),
        chains: Option<(
            Box<dyn SmartContractSignatureVerifier>,
            std::sync::Arc<dyn ChainBlocks>,
        )>,
    ) -> TestResult<Self> {
        let database = TestDatabase::new()?;
        let mut config = config_for(database.url());
        change(&mut config);
        let config = config.validate()?;
        if let Some(replica_url) = &config.database.replica_url {
            database.wait_for_replica(replica_url).await?;
        }
        let mut backend = server::initialize(config).await?;
        if let Some((verifier, chain)) = chains {
            backend.chains = chain;
            backend.verifier = std::sync::Arc::new(CachedSmartContractSignatureVerifier::new(
                verifier,
                backend.config.scw_cache_entries(),
            )?);
        }
        let running = RunningServer::from_backend(backend).await?;
        Ok(Self { running, database })
    }

    /// Stop the server and remove the database. The database is removed even
    /// when the server stops with an error, so a failed stop cannot leak it.
    pub async fn stop(mut self) -> TestResult {
        let stopped = self.running.stop().await;
        let removed = self.database.remove();
        stopped?;
        removed
    }
}

pub fn query_topic(topic: api::Topic, sequence_id: u64) -> api::TopicQuery {
    api::TopicQuery {
        topic: Some(topic),
        cursor: Some(api::Cursor { sequence_id }),
    }
}

pub fn topic(kind: xmtp_proto::types::TopicKind, identifier: &[u8]) -> api::Topic {
    api::Topic {
        topic: kind.create(identifier).to_vec(),
    }
}

impl RunningServer {
    /// Start an independent backend instance against an existing test database.
    pub async fn new(config: ValidatedConfig) -> TestResult<Self> {
        Self::from_backend(server::initialize(config).await?).await
    }

    /// Start a second backend on this server's database with `change` applied
    /// to the file layer of its configuration and validated again, as an
    /// operator restart with a new setting gives.
    pub async fn restart_with(&self, change: impl FnOnce(&mut Config)) -> TestResult<Self> {
        let mut config = (*self.backend.config).clone().into_config();
        change(&mut config);
        Self::new(config.validate()?).await
    }

    async fn from_backend(backend: Backend) -> TestResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(server::serve(backend.clone(), listener, async {
            let _ = stopped.await;
        }));
        let channel = Endpoint::from_shared(url.clone())?.connect().await?;
        Ok(Self {
            backend,
            channel,
            url,
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub fn configuration(
        &self,
    ) -> api::configuration_service_client::ConfigurationServiceClient<Channel> {
        api::configuration_service_client::ConfigurationServiceClient::new(self.channel.clone())
    }

    pub fn attachments(&self) -> api::attachment_service_client::AttachmentServiceClient<Channel> {
        api::attachment_service_client::AttachmentServiceClient::new(self.channel.clone())
    }

    pub fn query(&self) -> api::query_service_client::QueryServiceClient<Channel> {
        api::query_service_client::QueryServiceClient::new(self.channel.clone())
    }

    pub fn publisher(&self) -> api::publish_service_client::PublishServiceClient<Channel> {
        api::publish_service_client::PublishServiceClient::new(self.channel.clone())
    }

    pub fn notifications(
        &self,
    ) -> api::notification_service_client::NotificationServiceClient<Channel> {
        api::notification_service_client::NotificationServiceClient::new(self.channel.clone())
    }

    pub fn identity(&self) -> api::identity_service_client::IdentityServiceClient<Channel> {
        api::identity_service_client::IdentityServiceClient::new(self.channel.clone())
    }

    pub async fn publish(
        &self,
        envelopes: Vec<api::ClientEnvelope>,
    ) -> TestResult<Vec<api::EnvelopeMeta>> {
        Ok(self
            .publisher()
            .publish(api::PublishRequest { envelopes })
            .await?
            .into_inner()
            .envelope_metas)
    }

    /// Move every envelope on the identity topic of `inbox_id`, as lowercase
    /// hex, back in server time by `by`, so a test passes the interval between
    /// handle claims without waiting. Only `server_ns` changes: identity
    /// envelopes have no expiry, and sequence ids keep their order.
    pub async fn age_identity_history(
        &self,
        inbox_id: &str,
        by: std::time::Duration,
    ) -> TestResult {
        let topic = xmtp_proto::types::TopicKind::IdentityUpdatesV1.create(hex::decode(inbox_id)?);
        let by_ns = i64::try_from(by.as_nanos())?;
        sqlx::query("UPDATE envelopes SET server_ns = server_ns - $2 WHERE topic = $1")
            .bind(topic.to_vec())
            .bind(by_ns)
            .execute(&self.backend.store.primary)
            .await?;
        Ok(())
    }

    pub fn shutdown(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }

    /// Wait for service work without closing pools needed by database assertions.
    pub async fn wait_stopped(&mut self) -> TestResult {
        if let Some(task) = &mut self.task {
            let result = task.await;
            self.task = None;
            result??;
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> TestResult {
        self.shutdown();
        self.wait_stopped().await?;
        self.backend.store.primary.close().await;
        self.backend.store.read.close().await;
        Ok(())
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
