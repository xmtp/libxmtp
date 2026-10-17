//! Native test client for the worktree's Toxiproxy control API.
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;
use tokio::sync;

static TOXIPROXY_TEST_LOCK: LazyLock<sync::Mutex<()>> = LazyLock::new(|| sync::Mutex::new(()));
const TOXIPROXY_API_URL_DEFAULT: &str = "http://127.0.0.1:8474";
const CONTROL_TIMEOUT: crate::time::Duration = crate::time::Duration::from_secs(10);

/// A control client that never reads system or environment proxy settings.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::Client,
    base: reqwest::Url,
}

impl Client {
    fn new(url: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: crate::http::client_builder()
                .no_proxy()
                .timeout(CONTROL_TIMEOUT)
                .build()?,
            base: url.parse().expect("Toxiproxy control URL is valid"),
        })
    }

    fn url(&self, segments: &[&str]) -> reqwest::Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("Toxiproxy URL has a host")
            .pop_if_empty()
            .extend(segments);
        url
    }

    /// Enable every proxy and remove every toxic.
    pub async fn reset(&self) -> Result<(), reqwest::Error> {
        self.http
            .post(self.url(&["reset"]))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Fetch an existing proxy by name.
    pub async fn find_proxy(&self, name: &str) -> Result<Proxy, reqwest::Error> {
        let proxy: ProxyInfo = self
            .http
            .get(self.url(&["proxies", name]))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(Proxy {
            name: proxy.name,
            client: self.clone(),
        })
    }
}

#[derive(Deserialize)]
struct ProxyInfo {
    name: String,
}

#[derive(Deserialize)]
struct ToxicInfo {
    name: String,
}

/// A handle to one existing proxy. State is read from the server when needed.
#[derive(Debug)]
pub struct Proxy {
    name: String,
    client: Client,
}

impl Proxy {
    fn url(&self) -> reqwest::Url {
        self.client.url(&["proxies", &self.name])
    }

    async fn set_enabled(&self, enabled: bool) -> Result<(), reqwest::Error> {
        self.client
            .http
            .post(self.url())
            .json(&json!({ "enabled": enabled }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Disable the proxy and sever its active connections.
    pub async fn disable(&self) -> Result<(), reqwest::Error> {
        self.set_enabled(false).await
    }

    /// Enable the proxy again.
    pub async fn enable(&self) -> Result<(), reqwest::Error> {
        self.set_enabled(true).await
    }

    /// Add a timeout toxic. A zero timeout holds data until the toxic is removed.
    pub async fn with_timeout(
        &self,
        stream: String,
        timeout: u32,
        toxicity: f32,
    ) -> Result<(), reqwest::Error> {
        self.client
            .http
            .post(self.client.url(&["proxies", &self.name, "toxics"]))
            .json(&json!({
                "name": format!("timeout_{stream}"), "type": "timeout",
                "stream": stream, "toxicity": toxicity,
                "attributes": { "timeout": timeout }
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Fetch and remove all toxics, including those added by another handle.
    pub async fn delete_all_toxics(&self) -> Result<(), reqwest::Error> {
        let toxics: Vec<ToxicInfo> = self
            .client
            .http
            .get(self.client.url(&["proxies", &self.name, "toxics"]))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        for toxic in toxics {
            self.client
                .http
                .delete(
                    self.client
                        .url(&["proxies", &self.name, "toxics", &toxic.name]),
                )
                .send()
                .await?
                .error_for_status()?;
        }
        Ok(())
    }
}

static TOXIPROXY: LazyLock<Client> = LazyLock::new(|| {
    let url = std::env::var("XMTP_TOXIPROXY_API")
        .unwrap_or_else(|_| TOXIPROXY_API_URL_DEFAULT.to_string());
    Client::new(&url).expect("Toxiproxy control client builds")
});

/// The Toxiproxy control client for this worktree.
pub fn toxiproxy() -> &'static Client {
    &TOXIPROXY
}

/// Serialize proxy tests within a process and reset before each test.
/// Nextest callers must also run proxy tests with one test thread.
pub async fn toxiproxy_test<T, F: AsyncFn() -> T>(f: F) -> T {
    let _g = TOXIPROXY_TEST_LOCK.lock().await;
    TOXIPROXY.reset().await.unwrap();
    f().await
}
