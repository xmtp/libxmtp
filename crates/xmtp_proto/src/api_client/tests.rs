use futures::{StreamExt, stream};
use toxiproxy_rust::proxy::Proxy;

pub trait XmtpTestClient {
    type Builder;
    fn create() -> Self::Builder;
}

// _note:_ cannot use async fn in native b/c it creates lifetime errors in
// tester
#[xmtp_common::async_trait]
pub trait ToxicTestClient {
    /// returns all proxies relevant to this client
    async fn proxies() -> ToxicProxies;
}

pub struct ToxicProxies {
    /// Tuple of proxy and the port it is on
    inner: Vec<Proxy>,
}

impl ToxicProxies {
    pub fn new(proxies: impl Into<Vec<Proxy>>) -> Self {
        Self {
            inner: proxies.into(),
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.inner.extend(other.inner)
    }

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
}
