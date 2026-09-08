use crate::{Backend, api, config::Config, server};
mod database;
pub use database::TestDatabase;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tonic::transport::{Channel, Endpoint};
use xmtp_id::scw_verifier::{CachedSmartContractSignatureVerifier, SmartContractSignatureVerifier};

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct TestServer {
    pub backend: Backend,
    pub channel: Channel,
    pub url: String,
    database: TestDatabase,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}

impl TestServer {
    pub async fn new(change: impl FnOnce(&mut Config)) -> TestResult<Self> {
        Self::start(change, None).await
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
            database,
            stop: Some(stop),
            task,
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

    pub async fn stop(mut self) -> TestResult {
        self.stop.take().expect("one stop sender").send(()).ok();
        (&mut self.task).await??;
        self.backend.store.primary.close().await;
        self.backend.store.read.close().await;
        self.database.remove()?;
        Ok(())
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

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
    }
}
