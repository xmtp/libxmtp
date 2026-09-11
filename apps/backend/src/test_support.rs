use crate::{Backend, api, config::Config, server};
pub mod auth;
mod database;
#[cfg(test)]
pub(crate) mod metrics;
pub use database::TestDatabase;
// Only `apps/backend`'s own tests use these. Keep them out of the
// `test-utils` surface that `xmtp_mls` consumes.
#[cfg(test)]
pub(crate) mod grpc_web;
#[cfg(test)]
pub(crate) mod native;
#[cfg(test)]
pub(crate) mod replica;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tonic::transport::{Channel, Endpoint};
use xmtp_id::scw_verifier::{CachedSmartContractSignatureVerifier, SmartContractSignatureVerifier};

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
        let mut value: toml::Table = toml::from_str(toml)?;
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
        let mut value = toml::Value::Table(value);
        crate::config::resolve_environment(&mut value)?;
        let mut config: Config = value.try_into()?;
        config.validate()?;
        Self::start(
            move |defaults| {
                config.database.url = defaults.database.url.clone();
                *defaults = config;
            },
            None,
        )
        .await
    }

    pub async fn with_verifier(
        change: impl FnOnce(&mut Config),
        verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> TestResult<Self> {
        Self::start(change, Some(Box::new(verifier))).await
    }

    async fn start(
        change: impl FnOnce(&mut Config),
        verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
    ) -> TestResult<Self> {
        let database = TestDatabase::new()?;
        let mut config: Config =
            toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
        change(&mut config);
        let mut backend = server::initialize(config).await?;
        if let Some(verifier) = verifier {
            backend.verifier = std::sync::Arc::new(CachedSmartContractSignatureVerifier::new(
                verifier,
                std::num::NonZeroUsize::new(backend.config.validation.max_scw_cache_entries)
                    .unwrap(),
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
    pub async fn new(config: Config) -> TestResult<Self> {
        Self::from_backend(server::initialize(config).await?).await
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

    pub fn query(&self) -> api::query_service_client::QueryServiceClient<Channel> {
        api::query_service_client::QueryServiceClient::new(self.channel.clone())
    }

    pub fn publisher(&self) -> api::publish_service_client::PublishServiceClient<Channel> {
        api::publish_service_client::PublishServiceClient::new(self.channel.clone())
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
