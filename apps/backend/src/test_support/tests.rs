use super::{config_for, partial_config};
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
