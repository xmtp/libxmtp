//! What a publish is charged, and that a denied step stores nothing.

use super::transport::{PUBLISH, empty_envelopes};
use super::*;
use crate::{
    config::{CallerKey, Category},
    server::rate_limit::Charger,
    test_support::{native::envelope, rate_limit_delay, rate_limits},
};
use support::native::{framed, raw_unary};
use xmtp_mls_validation::test_utils::scw_create_inbox_update;

/// A server that charges each forwarded address as its own caller and
/// admits at most `max_publish_topics` topics in a publish.
async fn server(limits: &str, max_publish_topics: usize) -> support::TestResult<TestServer> {
    TestServer::new(|config| {
        config.limits.max_publish_topics = max_publish_topics;
        config.rate_limits = rate_limits(&format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n{limits}"
        ));
    })
    .await
}

/// A charger for the caller at `address`, to read what it has left.
fn caller(server: &TestServer, address: &str) -> Charger {
    let limiter = server
        .backend
        .rate_limiter
        .as_ref()
        .expect("rate limits on");
    Charger::for_test(limiter, CallerKey::from_ip(address.parse().unwrap()))
}

async fn publish_from(
    server: &TestServer,
    address: &str,
    envelopes: Vec<api::ClientEnvelope>,
) -> Result<api::PublishResponse, tonic::Status> {
    let mut request = tonic::Request::new(api::PublishRequest { envelopes });
    request
        .metadata_mut()
        .insert("x-forwarded-for", address.parse().unwrap());
    server
        .publisher()
        .publish(request)
        .await
        .map(tonic::Response::into_inner)
}

/// Spend exactly `left` units of `category`, and show none is left after.
async fn assert_left(caller: &Charger, category: Category, left: usize) {
    caller.charge(category, left).await.unwrap();
    assert!(
        caller.charge(category, 1).await.is_err(),
        "more than {left} {} left",
        category.as_str()
    );
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_is_charged_by_the_request_as_received() {
    let ten = "[{ count = 10, window_seconds = 3600 }]";
    let server = server(
        &format!(
            "[categories.publish]\nip = {ten}\n\
         [categories.identity_update]\nip = {ten}\n\
         [categories.chain_verify]\nip = {ten}"
        ),
        10,
    )
    .await?;
    const ADDRESS: &str = "203.0.113.1";
    // One identity update with one ERC-6492 signature, and three welcomes of
    // which two are the same. No chain is configured, so the update fails
    // validation after it is charged.
    let _ = publish_from(
        &server,
        ADDRESS,
        vec![
            identity_envelope(scw_create_inbox_update()),
            envelope(1, 1),
            envelope(1, 1),
            envelope(2, 1),
        ],
    )
    .await;
    // A publish with no envelopes costs one.
    publish_from(&server, ADDRESS, vec![]).await?;
    let caller = caller(&server, ADDRESS);
    assert_left(&caller, Category::IdentityUpdate, 9).await;
    assert_left(&caller, Category::ChainVerify, 9).await;
    assert_left(&caller, Category::Publish, 5).await;
    server.stop().await?;
}

// verifies: RATE-010
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_denied_by_a_charge_stores_nothing() {
    let fixture = identity_history_with_passkey().await;
    let server = server(
        &format!(
            "[categories.identity_update]\nip = [{{ count = {}, window_seconds = 3600 }}]\n\
         [categories.publish]\nip = [{{ count = 1, window_seconds = 3600 }}]\n\
         [[overrides]]\nip = \"203.0.113.1\"\n[overrides.limits]\npublish = []",
            fixture.history.len()
        ),
        10,
    )
    .await?;
    // Identity updates: the caller's whole history, then one update more.
    for update in &fixture.history {
        publish_from(
            &server,
            "203.0.113.1",
            vec![identity_envelope(update.clone())],
        )
        .await?;
    }
    let denied = publish_from(
        &server,
        "203.0.113.1",
        vec![identity_envelope(fixture.update.clone())],
    )
    .await;
    assert!(rate_limit_delay(&denied, Category::IdentityUpdate).is_some());
    let topic = xmtp_proto::types::TopicKind::IdentityUpdatesV1
        .create(hex::decode(&fixture.inbox_id)?)
        .to_vec();
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes WHERE topic = $1")
        .bind(&topic)
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(stored as usize, fixture.history.len());
    // Envelopes: one, then a second refused before it is stored.
    publish_from(&server, "203.0.113.2", vec![envelope(1, 1)]).await?;
    let denied = publish_from(&server, "203.0.113.2", vec![envelope(2, 1)]).await;
    assert!(rate_limit_delay(&denied, Category::Publish).is_some());
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes WHERE topic <> $1")
        .bind(&topic)
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(stored, 1);
    server.stop().await?;
}

/// An identity update with two ERC-6492 signatures.
fn two_signature_update() -> xmtp_proto::xmtp::identity::associations::IdentityUpdate {
    let mut update = scw_create_inbox_update_at(1);
    update
        .actions
        .extend(scw_change_recovery_update_at(1).actions);
    update
}

/// A publish over one bound of API section 7, as sent.
enum OverBound {
    /// A raw gRPC body, for the bounds checked before decode.
    Body(Vec<u8>),
    Envelopes(Vec<api::ClientEnvelope>),
}

impl OverBound {
    async fn send(&self, server: &TestServer, address: &str) -> support::TestResult<tonic::Status> {
        Ok(match self {
            Self::Body(body) => {
                raw_unary(
                    server,
                    PUBLISH,
                    &[("x-forwarded-for", address)],
                    body.clone(),
                    true,
                )
                .await?
            }
            Self::Envelopes(envelopes) => publish_from(server, address, envelopes.clone())
                .await
                .expect_err("the publish is over a bound"),
        })
    }
}

const BOUND_LIMITS: &str = "[categories.request]\nip = [{ count = 2, window_seconds = 3600 }]\n\
     [categories.publish]\nip = [{ count = 2, window_seconds = 3600 }]\n\
     [categories.identity_update]\nip = [{ count = 2, window_seconds = 3600 }]\n\
     [categories.chain_verify]\nip = [{ count = 2, window_seconds = 3600 }]";

/// Each bound of a publish is decided before the handler's charges. With
/// allowance left, the publish fails with `REASON_TOO_LARGE` having spent
/// its `request` unit and nothing else; with no `publish`,
/// `identity_update` or `chain_verify` allowance left, it still gets the
/// bound's status, not a rate limit.
// verifies: RATE-010, RATE-014, API-223, API-302
#[xmtp_common::test(unwrap_try = true)]
async fn every_publish_bound_is_decided_before_the_charges() {
    let oversized = envelope(1, 1);
    let actions = api::ClientEnvelope {
        payload: Some(api::client_envelope::Payload::IdentityUpdate(
            xmtp_proto::xmtp::identity::associations::IdentityUpdate {
                actions: vec![
                    Default::default();
                    xmtp_configuration::MAX_PUBLISH_IDENTITY_ACTIONS + 1
                ],
                ..Default::default()
            },
        )),
    };
    let message = api::PublishRequest {
        envelopes: vec![envelope(1, 1)],
    }
    .encode_to_vec();
    let small_envelopes = TestServer::new(|config| {
        config.limits.max_envelope_bytes = oversized.encoded_len() - 1;
        config.rate_limits = rate_limits(&format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n{BOUND_LIMITS}"
        ));
    })
    .await?;
    let small_counts = TestServer::new(|config| {
        config.limits.max_publish_topics = 1;
        config.limits.max_scw_signatures = 1;
        config.rate_limits = rate_limits(&format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n{BOUND_LIMITS}"
        ));
    })
    .await?;
    let cases = [
        (
            "envelope bytes",
            &small_envelopes,
            OverBound::Envelopes(vec![oversized]),
            Some(0),
        ),
        (
            "ERC-6492 signatures",
            &small_counts,
            OverBound::Envelopes(vec![identity_envelope(two_signature_update())]),
            Some(0),
        ),
        (
            "topics",
            &small_counts,
            OverBound::Envelopes(vec![envelope(1, 1), envelope(2, 1)]),
            None,
        ),
        (
            "envelope count",
            &small_counts,
            OverBound::Body(empty_envelopes(
                xmtp_configuration::MAX_PUBLISH_ENVELOPES + 1,
                &[],
            )),
            None,
        ),
        (
            "identity actions",
            &small_counts,
            OverBound::Envelopes(vec![actions]),
            None,
        ),
        (
            "messages",
            &small_counts,
            OverBound::Body([framed(&message), framed(&message)].concat()),
            None,
        ),
    ];
    for (number, (bound, server, request, index)) in cases.iter().enumerate() {
        let with_allowance = format!("203.0.113.{}", 2 * number + 1);
        let status = request.send(server, &with_allowance).await?;
        assert_eq!(status.code(), Code::InvalidArgument, "{bound}: {status:?}");
        let reason = tonic_types::pb::Status::decode(status.details())?
            .details
            .first()
            .map(|detail| api::PublishError::decode(detail.value.as_slice()))
            .transpose()?
            .ok_or(*bound)?;
        assert_eq!(reason.reason(), Reason::TooLarge, "{bound}");
        assert_eq!(reason.index, *index, "{bound}");
        let spent = caller(server, &with_allowance);
        assert_left(&spent, Category::Request, 1).await;
        assert_left(&spent, Category::Publish, 2).await;
        assert_left(&spent, Category::IdentityUpdate, 2).await;
        assert_left(&spent, Category::ChainVerify, 2).await;

        let exhausted = format!("203.0.113.{}", 2 * number + 2);
        let empty = caller(server, &exhausted);
        for category in [
            Category::Publish,
            Category::IdentityUpdate,
            Category::ChainVerify,
        ] {
            empty.charge(category, 2).await?;
        }
        let status = request.send(server, &exhausted).await?;
        assert_eq!(status.code(), Code::InvalidArgument, "{bound}: {status:?}");
    }
    small_envelopes.stop().await?;
    small_counts.stop().await?;
}

/// A request with an envelope over a bound is refused before its charges
/// with the lowest index of an envelope over a bound or one that does not
/// parse, and spends nothing but its `request` unit.
// verifies: API-223, RATE-010
#[xmtp_common::test(unwrap_try = true)]
async fn a_bound_failure_reports_the_lowest_envelope_found_failing() {
    let oversized = envelope(1, 1);
    let server = TestServer::new(|config| {
        config.limits.max_envelope_bytes = oversized.encoded_len() - 1;
        config.rate_limits = rate_limits(&format!(
            "max_memory_mib = 1\ntrusted_proxies = [\"127.0.0.1/32\"]\n{BOUND_LIMITS}"
        ));
    })
    .await?;
    let malformed = api::ClientEnvelope::default();
    for (address, envelopes, reason) in [
        (
            "203.0.113.1",
            vec![malformed.clone(), oversized.clone()],
            Reason::MalformedPayload,
        ),
        (
            "203.0.113.2",
            vec![oversized.clone(), malformed.clone()],
            Reason::TooLarge,
        ),
    ] {
        let status = publish_from(&server, address, envelopes).await.unwrap_err();
        let detail = api::PublishError::decode(
            tonic_types::pb::Status::decode(status.details())?.details[0]
                .value
                .as_slice(),
        )?;
        assert_eq!((detail.index, detail.reason()), (Some(0), reason));
        assert_left(&caller(&server, address), Category::Publish, 2).await;
    }
    server.stop().await?;
}
