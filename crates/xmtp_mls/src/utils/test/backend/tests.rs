use super::EphemeralBackend;
use crate::{context::XmtpSharedContext, groups::send_message_opts::SendMessageOpts, tester};
use futures::FutureExt;
use std::{
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use xmtp_api_backend::{AuthCallback, Credential};
use xmtp_backend::{config::ConfigError, test_support::TestDatabase};
use xmtp_common::{
    BoxDynError,
    time::{Duration, Instant},
};

fn database_name(backend: &EphemeralBackend) -> String {
    backend
        .config()
        .database
        .url
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_applies_partial_config_and_connects_tester() {
    let backend = EphemeralBackend::start(
        r#"
        [server]
        request_logger = false
        [database]
        max_connections = 3
        [limits]
        default_query_limit = 7
    "#,
    )
    .await?;
    let name = database_name(&backend);
    assert!(TestDatabase::names().await?.contains(&name));
    assert!(backend.url().starts_with("http://127.0.0.1:"));
    assert!(!backend.url().ends_with(":0"));
    assert!(!backend.config().server.request_logger);
    assert_eq!(backend.config().database.max_connections, 3);
    assert_eq!(backend.config().limits.default_query_limit, 7);
    assert_eq!(backend.config().streams.poll_interval_ms, 100);
    tester!(alix, backend: &backend);
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"ephemeral", SendMessageOpts::default())
        .await?;
    assert!(
        !alix
            .context
            .api()
            .query_group_messages(group.group_id)
            .await?
            .is_empty()
    );
    drop(alix);
    backend.stop().await?;
    assert!(!TestDatabase::names().await?.contains(&name));
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_drop_cleans_up_after_panic() {
    for panic in [false, true] {
        let backend = EphemeralBackend::start("").await?;
        let name = database_name(&backend);
        let address = backend.url().trim_start_matches("http://").to_owned();
        let started = Instant::now();
        let outcome = AssertUnwindSafe(async move {
            let _backend = backend;
            assert!(!panic, "intentional ephemeral test panic");
        })
        .catch_unwind()
        .await;
        assert_eq!(outcome.is_err(), panic);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!TestDatabase::names().await?.contains(&name));
        xmtp_common::wait_for_eq(
            || async { tokio::net::TcpStream::connect(&address).await.is_err() },
            true,
        )
        .await?;
    }
}

/// Change the process environment only in a child. Threaded tests stay isolated.
fn child_with_unreachable_database(name: &str) -> Option<std::process::Output> {
    if std::env::var("XMTP_EPHEMERAL_CHILD").as_deref() == Ok(name) {
        return None;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    Some(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("XMTP_EPHEMERAL_CHILD", name)
            .env("XMTP_EPHEMERAL_LISTEN", "127.0.0.1:0")
            .env(
                "DATABASE_URL",
                format!("postgres://xmtp:xmtp@{address}/unreachable"),
            )
            .output()
            .unwrap(),
    )
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_unreachable_database_fails_the_test() {
    let name = concat!(
        module_path!(),
        "::ephemeral_unreachable_database_fails_the_test"
    );
    // libtest names omit the crate prefix.
    let name = name.strip_prefix("xmtp_mls::").unwrap();
    if let Some(output) = child_with_unreachable_database(name) {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Connection refused"), "{stderr}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 failed"));
    } else {
        // The connection error must reach the test runner instead of skipping.
        EphemeralBackend::start("[server]\nlisten = 'env:XMTP_EPHEMERAL_LISTEN'").await?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_invalid_config_precedes_database_creation() {
    let name = concat!(
        module_path!(),
        "::ephemeral_invalid_config_precedes_database_creation"
    );
    let name = name.strip_prefix("xmtp_mls::").unwrap();
    if let Some(output) = child_with_unreachable_database(name) {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    } else {
        let error = EphemeralBackend::start("[database]\nmax_connections = 0")
            .await
            .err()
            .expect("invalid config");
        assert!(matches!(
            error.downcast_ref::<ConfigError>(),
            Some(ConfigError::Invalid {
                field: "database.max_connections",
                ..
            })
        ));
    }
}

/// The fixture owns the database URLs. A caller that sets one gets an error
/// instead of a backend that quietly ignores the value.
#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_rejects_caller_supplied_database_urls() {
    for field in ["url", "replica_url"] {
        let toml = format!("[database]\n{field} = 'postgres://example/other'");
        let error = EphemeralBackend::start(&toml)
            .await
            .err()
            .expect("fixture owns the database urls");
        assert!(
            error.to_string().contains(&format!("database.{field}")),
            "{error}"
        );
    }
    // An unresolvable environment reference still reaches the caller.
    let error = EphemeralBackend::start("[server]\nlisten = 'env:XMTP_EPHEMERAL_MISSING'")
        .await
        .err()
        .expect("missing environment variable");
    assert!(
        error.to_string().contains("XMTP_EPHEMERAL_MISSING"),
        "{error}"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_parallel_backends_keep_envelopes_separate() {
    let (first, second) =
        futures::try_join!(EphemeralBackend::start(""), EphemeralBackend::start(""))?;
    assert_ne!(first.url(), second.url());
    assert_ne!(database_name(&first), database_name(&second));
    let (alix, bo) = futures::join!(
        async {
            tester!(alix, backend: &first);
            alix
        },
        async {
            tester!(bo, backend: &second);
            bo
        }
    );
    let alix_group = alix.create_group(None, None)?;
    let bo_group = bo.create_group(None, None)?;
    let (a, b) = futures::join!(
        alix_group.send_message(b"first", SendMessageOpts::default()),
        bo_group.send_message(b"second", SendMessageOpts::default())
    );
    a?;
    b?;
    for (owner, other, group) in [
        (&alix, &bo, alix_group.group_id),
        (&bo, &alix, bo_group.group_id),
    ] {
        assert!(
            !owner
                .context
                .api()
                .query_group_messages(group)
                .await?
                .is_empty()
        );
        assert!(
            other
                .context
                .api()
                .query_group_messages(group)
                .await?
                .is_empty()
        );
    }
}

struct Callback(AtomicUsize);
#[xmtp_common::async_trait]
impl AuthCallback for Callback {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Credential::new(
            None,
            "Bearer ephemeral-test".parse()?,
            i64::MAX,
        ))
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_url_transport_calls_auth_callback() {
    let backend = EphemeralBackend::start("").await?;
    let callback = Arc::new(Callback(AtomicUsize::new(0)));
    tester!(alix, backend: &backend, auth: callback.clone());
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"authenticated transport", SendMessageOpts::default())
        .await?;
    assert!(
        !alix
            .context
            .api()
            .query_group_messages(group.group_id)
            .await?
            .is_empty()
    );
    assert_eq!(callback.0.load(Ordering::SeqCst), 1);
}

/// `auth` and `backend` may be given in either order.
#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_auth_before_backend_still_calls_the_callback() {
    let backend = EphemeralBackend::start("").await?;
    let callback = Arc::new(Callback(AtomicUsize::new(0)));
    tester!(alix, auth: callback.clone(), backend: &backend);
    alix.create_group(None, None)?;
    assert_eq!(callback.0.load(Ordering::SeqCst), 1);
}
