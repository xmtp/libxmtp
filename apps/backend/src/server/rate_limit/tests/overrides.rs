//! Overrides and tiers apply to the caller key a request is charged to.
use super::*;
use crate::{
    api,
    test_support::{
        TestServer,
        auth::{TestKey, mint, valid_claims},
    },
};
use serde_json::json;

/// How many lookups in a row the caller of `bearer`, from `address` when it
/// names none, is admitted, up to 5.
async fn admitted(server: &TestServer, bearer: &str, address: Option<&str>) -> TestResult<usize> {
    for admitted in 0..5 {
        let mut request = authorized(api::GetInboxIdsRequest::default(), bearer)?;
        if let Some(address) = address {
            request
                .metadata_mut()
                .insert("x-forwarded-for", address.parse()?);
        }
        let result = server.identity().get_inbox_ids(request).await;
        if rate_limit_delay(&result, Category::Request).is_some() {
            return Ok(admitted);
        }
        result?;
    }
    Ok(5)
}

// verifies: RATE-025, RATE-003, RATE-001
#[xmtp_common::test(unwrap_try = true)]
async fn overrides_match_the_normalized_caller_key() {
    let key = TestKey::es256();
    let server = TestServer::new(|config| {
        config.auth = Some(key.auth_config());
        config.rate_limits = rate_limits(
            "max_memory_mib = 1\n\
             trusted_proxies = [\"127.0.0.1/32\"]\n\
             [categories.request]\n\
             ip = [{ count = 1, window_seconds = 3600 }]\n\
             jwt = [{ count = 1, window_seconds = 3600 }]\n\
             [[overrides]]\nip = \"2001:db8:1:2::1\"\n\
             [overrides.limits]\nrequest = [{ count = 3, window_seconds = 3600 }]\n\
             [[overrides]]\nip = \"198.51.100.7\"\n\
             [overrides.limits]\nrequest = [{ count = 3, window_seconds = 3600 }]\n\
             [[overrides]]\njwt_subject = \"user\"\n\
             [overrides.limits]\nrequest = [{ count = 3, window_seconds = 3600 }]",
        );
    })
    .await?;
    // A token without `sub` is charged to the forwarded client address.
    let anonymous = mint(&valid_claims(), &key);
    let subject = |sub: &str, iss: Option<&str>| {
        let mut claims = valid_claims();
        claims["sub"] = json!(sub);
        if let Some(iss) = iss {
            claims["iss"] = json!(iss);
        }
        mint(&claims, &key)
    };
    // Another address in the override's /64.
    assert_eq!(
        admitted(&server, &anonymous, Some("2001:db8:1:2:ffff::9")).await?,
        3
    );
    // The IPv4-mapped form of the override's IPv4 address.
    assert_eq!(
        admitted(&server, &anonymous, Some("::ffff:198.51.100.7")).await?,
        3
    );
    // An override without `jwt_issuer` names a token without `iss` only.
    assert_eq!(admitted(&server, &subject("user", None), None).await?, 3);
    assert_eq!(
        admitted(
            &server,
            &subject("user", Some("https://issuer.example")),
            None
        )
        .await?,
        1
    );
    // No override matches: the tier's limits apply.
    assert_eq!(
        admitted(&server, &anonymous, Some("2001:db8:1:3::1")).await?,
        1
    );
    assert_eq!(admitted(&server, &subject("other", None), None).await?, 1);
    server.stop().await?;
}
