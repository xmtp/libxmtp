// The fully-monomorphized async client future (SyncWorker::run_tasks over the
// concrete PgMlsDb/PgKeyStore context) overflows rustc's default layout query
// depth of 128. herald sets the same limit for the same type.
#![recursion_limit = "512"]
//! Test harness for the FULL async xmtp_mls client over Postgres.
//!
//! Unlike `xmtp_db_pg_tests`, which drives the storage layer in isolation, these
//! tests build a real [`xmtp_mls::Client`] on the sqlx/Postgres store and make it
//! do what a server (herald) makes it do: register an identity on an XMTP node,
//! create groups, add members, and exchange encrypted messages. That path is the
//! one the async work most needed coverage on — welcome processing, key-package
//! consumption and the commit-log key all run through the `XmtpMlsStorageProvider`
//! KV that the async track re-implemented over sqlx, and none of it is exercised
//! by the query-level tests.
//!
//! ## Running
//!
//! Both a scratch Postgres and a reachable XMTP node are required. The suite is
//! **skipped** (not failed) when `XMTP_ASYNCDB_PG_URL` is unset, so a bare
//! `cargo test` here is a clean no-op.
//!
//! ```text
//! # local node (`just backend up`) + the async-track scratch Postgres:
//! XMTP_ASYNCDB_PG_URL=postgres://xmtp:xmtp@127.0.0.1:55432/xmtp_asyncdb \
//!   cargo test -p xmtp_mls_pg_tests -- --nocapture
//! ```
//!
//! `XMTP_MLS_PG_TEST_GRPC` overrides the node endpoint (default: the native local
//! node, `http://localhost:5556`).

use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context as _, Result, anyhow};
use sqlx::Executor;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_db::prelude::QueryMigrations;
use xmtp_db::{PgDb, PgKeyStore, PgMlsDb};
use xmtp_id::InboxOwner;
use xmtp_id::associations::Identifier;
use xmtp_mls::WrappedXmtpApiClient;
use xmtp_mls::context::XmtpMlsLocalContext;
use xmtp_mls::cursor_store::SqliteCursorStore;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_proto::types::ApiIdentifier;

/// The concrete async client: the shared context parameterized over `PgMlsDb`
/// (the `XmtpDb` store) and `PgKeyStore` (the openmls crypto store), both over
/// one `PgDb` pool. Identical to herald's `XmtpClient`.
pub type XmtpClient =
    xmtp_mls::Client<Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, PgMlsDb, PgKeyStore>>>;

/// Node-sdk default inbox nonce (`generateInboxId` nonce = 1).
const INBOX_NONCE: u64 = 1;

/// A registered client plus the handles a test needs to assert against its
/// Postgres namespace.
pub struct RegisteredClient {
    pub client: Arc<XmtpClient>,
    pub db: PgDb,
    pub schema: String,
    pub inbox_id: String,
}

impl RegisteredClient {
    /// `count(*)` for a table in this client's schema. `search_path` is pinned to
    /// the schema on every pooled connection, so an unqualified name resolves
    /// there.
    pub async fn count(&self, table: &str) -> i64 {
        let mut c = self.db.conn().await.expect("connection");
        let row = sqlx::query(&format!("SELECT count(*) AS n FROM {table}"))
            .fetch_one(&mut *c)
            .await
            .unwrap_or_else(|e| panic!("counting {table} in {}: {e}", self.schema));
        row.get::<i64, _>("n")
    }
}

/// The scratch Postgres URL, or `None` when unset — the signal to skip.
pub fn pg_url() -> Option<String> {
    match std::env::var("XMTP_ASYNCDB_PG_URL") {
        Ok(u) if !u.is_empty() => Some(u),
        _ => None,
    }
}

/// The XMTP node gRPC endpoint. Defaults to the native local node
/// (`just backend up`); override with `XMTP_MLS_PG_TEST_GRPC`.
pub fn grpc_endpoint() -> String {
    std::env::var("XMTP_MLS_PG_TEST_GRPC")
        .unwrap_or_else(|_| "http://localhost:5556".to_string())
}

/// Initialize test logging once (idempotent).
pub fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// Skip-guard for a `#[tokio::test]`: returns the Postgres URL, or prints a skip
/// line and `return`s from the calling test when it is unset.
#[macro_export]
macro_rules! pg_url_or_skip {
    () => {
        match $crate::pg_url() {
            Some(url) => url,
            None => {
                eprintln!(
                    "XMTP_ASYNCDB_PG_URL unset — skipping async xmtp_mls Postgres integration test"
                );
                return;
            }
        }
    };
}

/// A `PgMlsDb` over a freshly-created, private Postgres schema.
///
/// The schema is dropped and recreated so each run starts clean, then libxmtp's
/// Postgres MLS schema is applied via the migration runner (its tracking table
/// lives in the same schema). `search_path` is set per-connection via
/// `after_connect` because the pool hands out a different connection per query.
async fn build_store(database_url: &str, schema: &str) -> Result<PgMlsDb> {
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .context("connecting to Postgres (schema admin)")?;
    admin
        .execute(format!("DROP SCHEMA IF EXISTS {schema} CASCADE").as_str())
        .await
        .with_context(|| format!("dropping stale schema {schema}"))?;
    admin
        .execute(format!("CREATE SCHEMA {schema}").as_str())
        .await
        .with_context(|| format!("creating schema {schema}"))?;
    admin.close().await;

    let owned_schema = schema.to_string();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |conn, _meta| {
            let owned_schema = owned_schema.clone();
            Box::pin(async move {
                conn.execute(format!("SET search_path TO {owned_schema}").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .context("connecting to Postgres (client pool)")?;

    let db = PgDb::new(pool);
    db.run_pending_migrations()
        .await
        .context("running libxmtp Postgres MLS migrations")?;

    Ok(PgMlsDb::new(db))
}

/// v3-only gRPC API client, sharing the client's `PgDb` as its cursor store
/// (`SqliteCursorStore` is generic over any `DbQuery`, despite the name).
fn build_api(db: &PgDb) -> Result<xmtp_mls::XmtpApiClient> {
    let mut backend = MessageBackendBuilder::default();
    backend
        .v3_host(grpc_endpoint())
        .app_version("xmtp_mls_pg_tests/0".to_string())
        .cursor_store(SqliteCursorStore::new(db.clone()));
    Ok(backend.build_optional_d14n()?)
}

/// Resolve the inbox id for an identifier: ask the network first, fall back to
/// generating one locally. Mirrors node-sdk `getInboxIdForIdentifier` →
/// `generateInboxId`.
async fn resolve_inbox_id(api: &xmtp_mls::XmtpApiClient, identifier: &Identifier) -> Result<String> {
    let wrapper = ApiClientWrapper::new(api.clone(), strategies::exponential_cooldown());
    let api_identifier: ApiIdentifier = identifier.clone().into();
    let known = wrapper
        .get_inbox_ids(vec![api_identifier.clone()])
        .await
        .context("resolving inbox id from network")?;
    match known.get(&api_identifier) {
        Some(inbox_id) => Ok(inbox_id.clone()),
        None => Ok(identifier.inbox_id(INBOX_NONCE)?),
    }
}

/// The builder chain bindings/node runs, over the Postgres store.
async fn build_client(
    api: xmtp_mls::XmtpApiClient,
    store: PgMlsDb,
    inbox_id: String,
    identifier: Identifier,
) -> Result<XmtpClient> {
    let identity_strategy = IdentityStrategy::new(inbox_id, identifier, INBOX_NONCE, None);
    let client = xmtp_mls::Client::builder(identity_strategy)
        .api_client(api)
        .enable_api_stats()?
        .with_remote_verifier()?
        .store(store)
        .default_mls_store()?
        .build()
        .await?;
    Ok(client)
}

/// Register a fresh identity (random wallet, private Postgres schema) on the
/// configured XMTP node, and return the live client. Each call is independent:
/// unique key → unique inbox id → unique schema, so tests run in parallel and
/// re-run cleanly.
///
/// `label` names the schema (`t_<label>`); pass something unique per client per
/// test to avoid collisions.
pub async fn register_client(database_url: &str, label: &str) -> Result<RegisteredClient> {
    // The #[ctor] in xmtp_cryptography does not run on all platforms; install the
    // process-wide rustls provider explicitly. Idempotent.
    xmtp_cryptography::install_crypto_provider();

    let wallet_key: [u8; 32] = rand::random();
    let schema = format!("t_{label}_{}", hex::encode(&wallet_key[..5]));

    let signer = PrivateKeySigner::from_slice(&wallet_key)
        .map_err(|e| anyhow!("invalid wallet private key: {e}"))?;
    let identifier = signer.get_identifier()?;

    let store = build_store(database_url, &schema).await?;
    let db = store.pg().clone();
    let api = build_api(&db)?;
    let inbox_id = resolve_inbox_id(&api, &identifier).await?;
    let client = build_client(api, store, inbox_id.clone(), identifier).await?;

    // First boot for this identity: publish it, signing locally with the wallet
    // key (single-phase — both halves of the registrar flow inline).
    if let Some(mut signature_request) = client.context.signature_request() {
        let signature_text = signature_request.signature_text();
        let signature = signer.sign(&signature_text)?;
        signature_request
            .add_signature(signature, client.scw_verifier())
            .await?;
        client.register_identity(signature_request).await?;
    }

    let inbox_id = client.inbox_id().to_string();
    Ok(RegisteredClient {
        client: Arc::new(client),
        db,
        schema,
        inbox_id,
    })
}
