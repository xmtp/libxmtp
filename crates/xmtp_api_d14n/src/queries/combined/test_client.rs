//! `XmtpTestClient` impl that builds a real [`MigrationClient`] against a live
//! backend, so tests can exercise the v3↔d14n cutover client end to end (the
//! object production actually ships — see [`MigrationClient`]). The v3 side
//! dials xmtp-node-go; the d14n side is a [`ReadWriteClient`] over xmtpd +
//! gateway, matching the pure-d14n test client's transport shape.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use xmtp_configuration::PAYER_WRITE_FILTER;
use xmtp_proto::api::{Client, IsConnectedCheck};
use xmtp_proto::prelude::{ApiBuilder, XmtpTestClient};

use super::MigrationClient;
use crate::protocol::{CursorStore, NoCursorStore};
use crate::{ReadWriteClient, ReadWriteClientBuilder, XmtpTestClientExt};

/// Builder for a test [`MigrationClient`]: a v3 builder, a d14n read/write
/// builder, and the shared cursor store that carries the cutover state.
pub struct MigrationClientBuilder<V3B, D14nB, Store> {
    v3: V3B,
    xmtpd: D14nB,
    store: Store,
}

impl<V3B, D14nB, Store> MigrationClientBuilder<V3B, D14nB, Store> {
    pub fn new(v3: V3B, xmtpd: D14nB, store: Store) -> Self {
        Self { v3, xmtpd, store }
    }
}

impl<V3, R, W> XmtpTestClient for MigrationClient<V3, ReadWriteClient<R, W>, Arc<dyn CursorStore>>
where
    V3: XmtpTestClient,
    R: XmtpTestClient<Builder = W::Builder>,
    W: XmtpTestClient,
    W::Builder: Clone,
{
    type Builder = MigrationClientBuilder<
        V3::Builder,
        ReadWriteClientBuilder<R::Builder, W::Builder>,
        Arc<dyn CursorStore>,
    >;

    fn create() -> Self::Builder {
        let v3 = <V3 as XmtpTestClient>::create();
        let rw = ReadWriteClient::builder()
            .read(<R as XmtpTestClient>::create())
            .write(<W as XmtpTestClient>::create())
            .filter(PAYER_WRITE_FILTER);
        MigrationClientBuilder::new(v3, rw, Arc::new(NoCursorStore))
    }
}

impl<V3, R, W> XmtpTestClientExt
    for MigrationClient<V3, ReadWriteClient<R, W>, Arc<dyn CursorStore>>
where
    V3: XmtpTestClient,
    R: XmtpTestClient<Builder = W::Builder>,
    W: XmtpTestClient,
    W::Builder: Clone,
{
    fn with_cursor_store(store: Arc<dyn CursorStore>) -> <Self as XmtpTestClient>::Builder {
        let mut b = <Self as XmtpTestClient>::create();
        b.store = store;
        b
    }
}

impl<V3B, BRead, BWrite> ApiBuilder
    for MigrationClientBuilder<V3B, ReadWriteClientBuilder<BRead, BWrite>, Arc<dyn CursorStore>>
where
    V3B: ApiBuilder,
    V3B::Output: Client + IsConnectedCheck + Clone + 'static,
    BRead: ApiBuilder,
    BWrite: ApiBuilder,
    ReadWriteClient<BRead::Output, BWrite::Output>: Client + IsConnectedCheck + Clone + 'static,
{
    type Output = MigrationClient<
        V3B::Output,
        ReadWriteClient<BRead::Output, BWrite::Output>,
        Arc<dyn CursorStore>,
    >;
    // The v3 grpc builder carries the only fallible-on-connect step; the
    // read/write build and `MigrationClient::new` (a local SCW-verifier setup)
    // are infallible in tests, so they unwrap.
    type Error = <V3B as ApiBuilder>::Error;

    fn build(self) -> Result<Self::Output, Self::Error> {
        let v3 = self.v3.build()?;
        let xmtpd =
            <ReadWriteClientBuilder<BRead, BWrite> as ApiBuilder>::build(self.xmtpd).unwrap();
        Ok(MigrationClient::new(v3, xmtpd, self.store).unwrap())
    }
}
