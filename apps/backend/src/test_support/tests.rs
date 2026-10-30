use super::{RunningServer, TestServer, config_for, partial_config};
use crate::config::{CallerKey, Category, LimitConfig};

/// Fixture configurations turn rate limits off, because every test client
/// shares one address. A partial configuration with a `[rate_limits]` table
/// applies it as written, so a test can turn limits on with low values.
#[xmtp_common::test(unwrap_try = true)]
fn fixtures_turn_rate_limits_off_unless_a_test_sets_them() {
    assert!(
        !config_for("postgres://localhost/xmtp")
            .validate()?
            .rate_limits()
            .enabled
    );
    for partial in ["", "[limits]\ndefault_query_limit = 7\n"] {
        assert!(
            !partial_config(partial)?.validate()?.rate_limits().enabled,
            "{partial:?}"
        );
    }
    let limited = partial_config(
        "[rate_limits.categories.publish]\ndefault = [{ count = 1, window_seconds = 60 }]\n",
    )?
    .validate()?;
    let limits = limited.rate_limits();
    assert!(limits.enabled);
    assert_eq!(
        limits.limits(
            Category::Publish,
            &CallerKey::from_ip([127, 0, 0, 1].into())
        ),
        [LimitConfig {
            count: 1,
            window_seconds: 60,
        }]
    );
    assert!(
        !partial_config("[rate_limits]\nenabled = false\nmax_memory_mib = 1\n")?
            .validate()?
            .rate_limits()
            .enabled
    );
}

/// A started fixture server runs with rate limits off. `RunningServer::new`,
/// which takes a configuration a test built itself, refuses one with limits
/// on, so a new harness cannot leave them on by omitting the table.
#[xmtp_common::test(unwrap_try = true)]
async fn started_test_servers_run_with_rate_limits_off() {
    let server = TestServer::new(|_| {}).await?;
    assert!(!server.backend.config.rate_limits().enabled);
    let mut config = (*server.backend.config).clone().into_config();
    config.rate_limits.enabled = true;
    let refused = RunningServer::new(config.validate()?).await.err()?;
    assert!(refused.to_string().contains("rate limits"), "{refused}");
    server.stop().await?;
}
