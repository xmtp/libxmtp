use super::*;
use futures::stream;
use futures::StreamExt;
use tokio::sync::OnceCell;
use toxiproxy_rust::proxy::Proxy;
use toxiproxy_rust::proxy::ProxyPack;
use xmtp_configuration::localhost_to_internal;
use xmtp_configuration::toxi_port;
use xmtp_configuration::DockerUrls;

static TOXIPROXY: OnceCell<toxiproxy_rust::client::Client> = OnceCell::const_new();

pub struct ToxicProxies {
    /// Tuple of proxy and the port it is on
    inner: Vec<Proxy>,
    ports: Vec<u16>,
}

impl ToxicProxies {
    pub fn proxies(&self) -> &[Proxy] {
        self.inner.as_ref()
    }

    pub fn proxy(&self, n: usize) -> &Proxy {
        &self.inner[n]
    }

    /// Apply a toxic to each proxy
    pub async fn for_each<F>(&self, f: F)
    where
        F: AsyncFn(&Proxy),
    {
        let _ = stream::iter(self.inner.iter())
            .for_each(|p| async {
                f(p).await;
            })
            .await;
    }

    pub fn ports(&self) -> &[u16] {
        self.ports.as_ref()
    }

    pub fn port(&self, n: usize) -> u16 {
        self.ports[n]
    }

    pub fn proxy_by_port(&self, port: u16) -> Option<&Proxy> {
        let idx = self
            .ports
            .iter()
            .enumerate()
            .find(|(_, &p)| p.eq(&port))
            .map(|(i, _)| i);
        idx.map(|i| &self.inner[i])
    }
}

/// Init a new Toxi combination.
/// Returns the proxy and the address to use for it.
pub async fn init_toxi(outgoing_addrs: &[&str]) -> ToxicProxies {
    let toxiproxy = TOXIPROXY
        .get_or_init(|| async {
            let toxiproxy = toxiproxy_rust::client::Client::new(DockerUrls::TOXIPROXY);
            toxiproxy.reset().await.unwrap();
            toxiproxy
        })
        .await;

    let ports = outgoing_addrs
        .iter()
        .map(|a| toxi_port(a))
        .collect::<Vec<u16>>();

    let proxies = toxiproxy
        .populate(
            stream::iter(outgoing_addrs.iter())
                .then(async |addr| {
                    let internal_host = localhost_to_internal(addr);
                    let port_num = toxi_port(addr);
                    tracing::info!("port number {port_num}");
                    let toxic_host = format!(
                        "{}:{}",
                        internal_host.host_str().unwrap(),
                        internal_host.port().unwrap()
                    );
                    tracing::info!("creating toxiproxy for host={toxic_host}");
                    ProxyPack::new(
                        format!("Proxy {}", internal_host.port().unwrap()),
                        format!("[::]:{port_num}"),
                        toxic_host,
                    )
                    .await
                })
                .collect()
                .await,
        )
        .await
        .unwrap();

    ToxicProxies {
        inner: proxies,
        ports,
    }
}

/// ApiBuilder with extra methods for use in tests
pub trait TestApiBuilder: ApiBuilder {
    /// Build the api with toxiyproxy
    /// Returns a proxy object for this toxiproxy layer
    /// Returns a proxy and the address for it
    #[allow(async_fn_in_trait)]
    async fn with_toxiproxy(&mut self) -> ToxicProxies;

    fn with_existing_toxi(&mut self, addr: &str) {
        self.set_host(addr.into());
    }
}
