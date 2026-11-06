//! What a stream spends for its caller: rent for the topics it holds, a
//! `request` unit for each read of the store it makes, and nothing for live
//! delivery. Every test runs the limiter on a clock it sets by hand; a client
//! `Ping` wakes a session to see a time the test set.
use crate::{
    api::{self, subscribe_request::Request as Input, subscribe_response::Response as Frame},
    config::{CallerKey, Category},
    server::rate_limit::Charger,
    test_support::{
        self as support, TestResult, TestServer,
        native::{Native, envelope},
        rate_limit_delay, rate_limits,
    },
};
use std::time::Duration;
use xmtp_proto::types::TopicKind;
use xmtp_rate_limit::ManualClock;

const LOCALHOST: CallerKey = CallerKey::Ipv4([127, 0, 0, 1]);

/// A server with `limits` under `[rate_limits]`, its limiter on `clock`.
async fn server(limits: &str, clock: &ManualClock) -> TestResult<TestServer> {
    server_with(limits, clock, |_| {}).await
}

/// `server`, with `change` applied to the rest of its configuration.
async fn server_with(
    limits: &str,
    clock: &ManualClock,
    change: impl FnOnce(&mut crate::config::Config),
) -> TestResult<TestServer> {
    TestServer::with_rate_limit_clock(
        |config| {
            config.rate_limits = rate_limits(&format!("max_memory_mib = 1\n{limits}"));
            change(config);
        },
        clock.clone(),
    )
    .await
}

fn identifier(index: usize) -> [u8; 32] {
    let mut identifier = [0; 32];
    identifier[..8].copy_from_slice(&(index as u64).to_be_bytes());
    identifier
}

/// The welcome topic numbered `index`.
fn topic(index: usize) -> api::Topic {
    support::topic(TopicKind::WelcomeMessagesV1, &identifier(index))
}

/// Queries from cursor 0 for the topics numbered `indexes`.
fn queries(indexes: std::ops::Range<usize>) -> Vec<api::TopicQuery> {
    indexes
        .map(|index| support::query_topic(topic(index), 0))
        .collect()
}

/// The cost of each spend in `category` the server decided after its first
/// `from` spends, and whether the limiter admitted it.
fn spends(server: &TestServer, from: usize, category: Category) -> Vec<(u32, bool)> {
    server.rate_limit_spends()[from..]
        .iter()
        .filter(|spend| spend.category == category)
        .map(|spend| (spend.cost, spend.retry_after.is_none()))
        .collect()
}

/// The retry delays of the denied spends in `category`.
fn denials(server: &TestServer, category: Category) -> Vec<Duration> {
    server
        .rate_limit_spends()
        .iter()
        .filter(|spend| spend.category == category)
        .filter_map(|spend| spend.retry_after)
        .collect()
}

/// Send `adds` and `removes` as update `id` and wait for its `Applied`.
async fn applied(
    stream: &mut Native,
    id: u64,
    adds: Vec<api::TopicQuery>,
    removes: Vec<api::Topic>,
) -> TestResult<api::subscribe_response::Applied> {
    stream.update(id, adds, removes).await?;
    match stream.next().await? {
        Frame::Applied(applied) if applied.id == id => Ok(applied),
        frame => Err(format!("expected Applied {id}, got {frame:?}").into()),
    }
}

/// Ping the session and wait for its `Pong`. The session handles the `Ping`
/// in a turn of its loop, so its next turn sees the time the test set.
async fn ping(stream: &mut Native, nonce: u64) -> TestResult {
    stream.send(Input::Ping(api::Ping { nonce })).await?;
    match stream.next().await? {
        Frame::Pong(pong) if pong.nonce == nonce => Ok(()),
        frame => Err(format!("expected Pong {nonce}, got {frame:?}").into()),
    }
}

/// Ping the session and collect the next `count` envelopes, whichever comes
/// first: a session that resumes catch-up on its own timer can send them
/// before the `Pong`.
async fn ping_for_messages(
    stream: &mut Native,
    nonce: u64,
    count: usize,
) -> TestResult<Vec<api::ServerEnvelope>> {
    stream.send(Input::Ping(api::Ping { nonce })).await?;
    let mut ponged = false;
    let mut rows = Vec::new();
    while !ponged || rows.len() < count {
        match stream.next().await? {
            Frame::Pong(pong) if pong.nonce == nonce && !ponged => ponged = true,
            Frame::Messages(messages) => rows.extend(messages.envelopes),
            frame => return Err(format!("unexpected frame: {frame:?}").into()),
        }
    }
    Ok(rows)
}

/// The status the stream ends with, which must come before any other frame.
async fn ended(stream: &mut Native) -> TestResult<tonic::Status> {
    match xmtp_common::time::timeout(Duration::from_secs(5), stream.output.message()).await? {
        Err(status) => Ok(status),
        Ok(frame) => Err(format!("expected the stream to end, got {frame:?}").into()),
    }
}

fn secs(seconds: u64) -> u64 {
    seconds * 1_000_000_000
}

// verifies: RATE-030
#[xmtp_common::test(unwrap_try = true)]
async fn a_stream_pays_rent_for_its_topics_at_open_and_each_period() {
    let clock = ManualClock::new();
    let server = server("", &clock).await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..2500), vec![]).await?;
    // One unit at open for no topics, then three more for 2500.
    assert_eq!(
        spends(&server, 0, Category::StreamRent),
        [(1, true), (3, true)]
    );
    for period in 1..=2 {
        clock.advance(secs(60));
        ping(&mut stream, period).await?;
        let mut expected = vec![(1, true), (3, true)];
        expected.extend((0..period).map(|_| (4, true)));
        xmtp_common::wait_for_eq(
            || async { spends(&server, 0, Category::StreamRent) },
            expected,
        )
        .await?;
    }
    drop(stream);
    server.stop().await?;
}

// verifies: RATE-031
#[xmtp_common::test(unwrap_try = true)]
async fn an_update_pays_only_the_rent_it_raises_before_its_applied() {
    let clock = ManualClock::new();
    let server = server("", &clock).await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..900), vec![]).await?;
    let paid = spends(&server, 0, Category::StreamRent);
    assert_eq!(paid, [(1, true), (1, true)]);
    // 1100 topics need a third unit.
    applied(&mut stream, 2, queries(900..1100), vec![]).await?;
    assert_eq!(
        spends(&server, 0, Category::StreamRent),
        [(1, true), (1, true), (1, true)]
    );
    // The period already paid for up to 2000.
    applied(&mut stream, 3, queries(1100..1200), vec![]).await?;
    assert_eq!(
        spends(&server, 0, Category::StreamRent),
        [(1, true), (1, true), (1, true)]
    );
    drop(stream);
    server.stop().await?;
}

/// An update the session handles after a period began pays that period's
/// rent first, for the topics the stream held when it began, and tops up
/// from it.
// verifies: RATE-030, RATE-031
#[xmtp_common::test(unwrap_try = true)]
async fn an_update_after_a_period_began_tops_up_from_its_renewal() {
    let clock = ManualClock::new();
    let server = server("", &clock).await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..900), vec![]).await?;
    // Nothing wakes the session until the update arrives.
    clock.advance(secs(60));
    applied(&mut stream, 2, queries(900..1100), vec![]).await?;
    ping(&mut stream, 1).await?;
    ping(&mut stream, 2).await?;
    // The second period pays 2 units for 900 topics, then 1 for 1100.
    assert_eq!(
        spends(&server, 0, Category::StreamRent),
        [(1, true), (1, true), (2, true), (1, true)]
    );
    drop(stream);
    server.stop().await?;
}

// verifies: RATE-030, RATE-032, RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn a_denied_renewal_ends_the_stream_with_the_rate_limit_rejection() {
    let Some(metrics) = support::metrics::isolated(
        "stream::tests::charges::a_denied_renewal_ends_the_stream_with_the_rate_limit_rejection",
    ) else {
        return;
    };
    let clock = ManualClock::new();
    let server = server(
        "[categories.stream_rent]\ndefault = [{ count = 2, window_seconds = 3600 }]",
        &clock,
    )
    .await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..1), vec![]).await?;
    clock.advance(secs(60));
    stream.send(Input::Ping(api::Ping { nonce: 1 })).await?;
    let status = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            match stream.output.message().await {
                Ok(Some(api::SubscribeResponse {
                    response: Some(Frame::Pong(_)),
                })) => continue,
                Ok(frame) => panic!("unexpected frame {frame:?}"),
                Err(status) => break status,
            }
        }
    })
    .await?;
    // Both units of the hour were spent at 0, so the next two fit at 3600 s.
    assert_eq!(
        rate_limit_delay(&Err::<(), _>(status.clone()), Category::StreamRent),
        Some(Duration::from_secs(3540))
    );
    assert!(!status.message().contains("127.0.0.1"));
    xmtp_common::wait_for_eq(
        || async {
            support::metrics::value(
                &metrics,
                "xmtp_stream_ended_total",
                &[("reason", "rate_limited")],
            )
        },
        1.0,
    )
    .await?;
    server.stop().await?;
}

// verifies: RATE-031, RATE-032
#[xmtp_common::test(unwrap_try = true)]
async fn a_denied_top_up_ends_the_stream_before_its_applied() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.stream_rent]\ndefault = [{ count = 1, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let mut stream = Native::open(&server).await?;
    stream.update(1, queries(0..1), vec![]).await?;
    let status = ended(&mut stream).await?;
    // The whole rent of 2 is more than the count of 1, so the delay runs to
    // full allowance, when one spend of 2 is admitted. A replacement that
    // opens for 1 unit and then tops up can never pay it.
    assert_eq!(
        rate_limit_delay(&Err::<(), _>(status), Category::StreamRent),
        Some(Duration::from_secs(60))
    );
    // The rent is paid first, so its denial leaves the catch-up target
    // unspent: only the `Subscribe` spent a `request` unit.
    assert_eq!(spends(&server, 0, Category::Request), [(1, true)]);
    server.stop().await?;
}

/// The review's case: a top-up denied with 40.5 s of backlog under 120 units
/// per 60 s. A replacement stream that waits only for the top-up opens and
/// is denied the same top-up again; the reported delay covers its open too.
// verifies: RATE-032
#[xmtp_common::test(unwrap_try = true)]
async fn a_replacement_stream_that_waits_the_rent_delay_is_admitted() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.stream_rent]\ndefault = [{ count = 120, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let limiter = server.backend.rate_limiter.clone().ok_or("limiter")?;
    // 80 units of other streams, and the open's unit: 40.5 s of backlog.
    Charger::for_test(&limiter, LOCALHOST)
        .charge(Category::StreamRent, 80)
        .await?;
    let mut stream = Native::open(&server).await?;
    // 100,000 topics need 101 units; this period paid 1.
    let update = queries(0..100_000);
    stream.update(1, update.clone(), vec![]).await?;
    let delay = rate_limit_delay(
        &Err::<(), _>(ended(&mut stream).await?),
        Category::StreamRent,
    )
    .ok_or("rent denial")?;
    clock.advance(delay.as_nanos() as u64 + 50_000_000);
    let mut replacement = Native::open(&server).await?;
    applied(&mut replacement, 1, update, vec![]).await?;
    // The delay for 101 units, not the 30.5 s of the denied 100.
    assert_eq!(delay, Duration::from_secs(31));
    drop(replacement);
    server.stop().await?;
}

// verifies: RATE-010, RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn each_catch_up_read_spends_one_request_unit() {
    let clock = ManualClock::new();
    let server = server("", &clock).await?;
    server
        .publish(
            (0..600)
                .map(|index| {
                    xmtp_mls_validation::test_utils::inline_welcome_envelope(identifier(index))
                })
                .collect(),
        )
        .await?;
    let from = server.rate_limit_spends().len();
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..600), vec![]).await?;
    assert_eq!(stream.messages(600).await?.len(), 600);
    // The `Subscribe`, the update's catch-up target, and three reads of at
    // most 256 registrations each.
    assert_eq!(spends(&server, from, Category::Request), [(1, true); 5]);
    drop(stream);
    server.stop().await?;
}

// A session that retried a denied read at once would spin without yielding,
// so a second worker keeps the test running to report it.
// verifies: RATE-033, API-254
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 2)]
async fn a_denied_catch_up_read_waits_its_delay_and_skips_nothing() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.request]\ndefault = [{ count = 2, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let metas = server
        .publish((0..3).map(|value| envelope(7, value)).collect())
        .await?;
    // The publish's unit is earned back.
    clock.advance(secs(60));
    let from = server.rate_limit_spends().len();
    let mut stream = Native::open(&server).await?;
    applied(
        &mut stream,
        1,
        vec![support::query_topic(
            metas[0].topic.clone().ok_or("topic")?,
            0,
        )],
        vec![],
    )
    .await?;
    // The `Subscribe` and the catch-up target spent both units, so the read
    // waits for one to be earned back.
    xmtp_common::wait_for_ge(|| async { denials(&server, Category::Request).len() }, 1).await?;
    assert_eq!(
        denials(&server, Category::Request)[0],
        Duration::from_secs(30)
    );
    clock.advance(secs(30) - 1_000_000);
    // The read stays paused: the session handles the second `Ping` only
    // after the turn that follows the first.
    ping(&mut stream, 1).await?;
    ping(&mut stream, 2).await?;
    assert_eq!(
        spends(&server, from, Category::Request),
        [(1, true), (1, true), (1, false)]
    );
    clock.advance(1_000_000);
    let rows = ping_for_messages(&mut stream, 3, 3).await?;
    let cursors: Vec<_> = rows
        .iter()
        .map(|row| row.meta.as_ref().and_then(|meta| meta.cursor))
        .collect();
    let published: Vec<_> = metas.iter().map(|meta| meta.cursor).collect();
    assert_eq!(cursors, published);
    assert_eq!(
        spends(&server, from, Category::Request),
        [(1, true), (1, true), (1, false), (1, true)]
    );
    // The stream stayed open throughout.
    ping(&mut stream, 4).await?;
    drop(stream);
    server.stop().await?;
}

// verifies: RATE-034, RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn an_update_that_newly_registers_a_topic_pays_for_its_catch_up_target() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.request]\ndefault = [{ count = 3, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..1), vec![]).await?;
    assert_eq!(spends(&server, 0, Category::Request), [(1, true); 2]);
    // A re-add of a registered topic, and a removal, read no target.
    applied(&mut stream, 2, queries(0..1), vec![]).await?;
    applied(&mut stream, 3, vec![], vec![topic(0)]).await?;
    assert_eq!(spends(&server, 0, Category::Request), [(1, true); 2]);
    // Adding the removed topic again reads its target again.
    applied(&mut stream, 4, queries(0..1), vec![]).await?;
    assert_eq!(spends(&server, 0, Category::Request), [(1, true); 3]);
    stream.update(5, queries(2..3), vec![]).await?;
    let status = ended(&mut stream).await?;
    // The 20 s one unit needs would let the replacement's `Subscribe` in and
    // deny its update again.
    let delay =
        rate_limit_delay(&Err::<(), _>(status), Category::Request).ok_or("request denial")?;
    assert_eq!(delay, Duration::from_secs(40));
    clock.advance(delay.as_nanos() as u64);
    let mut replacement = Native::open(&server).await?;
    applied(&mut replacement, 1, queries(2..3), vec![]).await?;
    drop(replacement);
    server.stop().await?;
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
async fn live_envelopes_arrive_while_the_caller_has_no_requests_left() {
    let clock = ManualClock::new();
    let server = server(
        "trusted_proxies = [\"127.0.0.1/32\"]\n\
         [categories.request]\nip = [{ count = 2, window_seconds = 3600 }]",
        &clock,
    )
    .await?;
    let limiter = server.backend.rate_limiter.clone().ok_or("limiter")?;
    let mut stream = Native::open(&server).await?;
    applied(
        &mut stream,
        1,
        vec![support::query_topic(topic(5), 0)],
        vec![],
    )
    .await?;
    // A denied spend charges nothing, so this only shows none is left.
    assert!(
        Charger::for_test(&limiter, LOCALHOST)
            .charge(Category::Request, 1)
            .await
            .is_err()
    );
    // Another caller, behind the trusted proxy, publishes.
    let mut request = tonic::Request::new(api::PublishRequest {
        envelopes: vec![xmtp_mls_validation::test_utils::inline_welcome_envelope(
            identifier(5),
        )],
    });
    request
        .metadata_mut()
        .insert("x-forwarded-for", "192.0.2.1".parse()?);
    server.publisher().publish(request).await?;
    assert_eq!(stream.messages(1).await?.len(), 1);
    // Only the probe was denied: the delivery spent nothing.
    assert_eq!(denials(&server, Category::Request).len(), 1);
    drop(stream);
    server.stop().await?;
}

// verifies: RATE-030, RATE-032
#[xmtp_common::test(unwrap_try = true)]
async fn a_static_stream_pays_its_whole_rent_at_open() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.stream_rent]\ndefault = [{ count = 2, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let open = || {
        let mut client = client.clone();
        async move {
            client
                .subscribe_static(api::SubscribeStaticRequest {
                    topics: queries(0..1500),
                })
                .await
        }
    };
    // 1500 topics need 3 units, more than the count: a caller with nothing
    // spent may pay them at once.
    let mut first = open().await?.into_inner();
    assert!(matches!(
        first.message().await?.and_then(|frame| frame.response),
        Some(api::subscribe_static_response::Response::Started(_))
    ));
    assert_eq!(spends(&server, 0, Category::StreamRent), [(3, true)]);
    // Static targets are read with the `SubscribeStatic` request's own unit.
    assert_eq!(spends(&server, 0, Category::Request), [(1, true)]);
    let refused = open().await.map(drop);
    assert_eq!(
        rate_limit_delay(&refused, Category::StreamRent),
        Some(Duration::from_secs(90))
    );
    // A native stream's open pays its one unit, which also waits.
    let refused = client
        .subscribe(futures::stream::pending::<api::SubscribeRequest>())
        .await
        .map(drop);
    assert_eq!(
        rate_limit_delay(&refused, Category::StreamRent),
        Some(Duration::from_secs(60))
    );
    drop(first);
    server.stop().await?;
}

/// A refused stream step counts one rejection, even when its delay comes
/// from a second, larger spend, and a refused update counts as one
/// rate-limited update, a static open's included.
// verifies: RATE-032
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn each_refused_stream_step_counts_once() {
    let Some(metrics) =
        support::metrics::isolated("stream::tests::charges::each_refused_stream_step_counts_once")
    else {
        return;
    };
    // Operation spans reach the metrics through the backend's logging.
    let _logging = xmtp_logging::XmtpLogging::builder()
        .level(xmtp_logging::Level::Info)
        .install()?;
    let metric =
        |name: &str, labels: &[(&str, &str)]| support::metrics::value(&metrics, name, labels);
    let rejections = || {
        metric(
            "xmtp_rate_limit_rejections_total",
            &[("category", "stream_rent")],
        )
    };
    let refused_updates = || metric("xmtp_stream_updates_total", &[("outcome", "rate_limited")]);
    let update_spans = || {
        metric(
            "xmtp_operation_duration_seconds_count",
            &[("operation", "stream.update")],
        )
    };
    let clock = ManualClock::new();
    let server = server(
        "[categories.stream_rent]\ndefault = [{ count = 2, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..1), vec![]).await?;
    // 1001 topics need 1 more unit, and its delay is read from a spend of 3.
    stream.update(2, queries(0..1001), vec![]).await?;
    ended(&mut stream).await?;
    assert_eq!(rejections(), 1.0);
    assert_eq!(refused_updates(), 1.0);
    drop(stream);
    clock.advance(secs(60));
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let open = || {
        let mut client = client.clone();
        async move {
            client
                .subscribe_static(api::SubscribeStaticRequest {
                    topics: queries(0..1),
                })
                .await
        }
    };
    let spans = update_spans();
    let first = open().await?;
    assert_eq!(update_spans(), spans + 1.0);
    assert!(rate_limit_delay(&open().await.map(drop), Category::StreamRent).is_some());
    assert_eq!(rejections(), 2.0);
    assert_eq!(refused_updates(), 2.0);
    // A refused open leaves no session behind.
    let refused = client
        .subscribe(futures::stream::pending::<api::SubscribeRequest>())
        .await
        .map(drop);
    assert!(rate_limit_delay(&refused, Category::StreamRent).is_some());
    assert_eq!(rejections(), 3.0);
    xmtp_common::wait_for_eq(
        || async { metric("xmtp_stream_sessions", &[("kind", "bidi")]) },
        0.0,
    )
    .await?;
    assert_eq!(metric("xmtp_stream_sessions", &[("kind", "static")]), 1.0);
    drop(first);
    server.stop().await?;
}

/// A paused read resumes on the session's own timer, with no frame from the
/// client, no keepalive, and no notice from the tailer to wake it.
// verifies: RATE-033
#[xmtp_common::test(unwrap_try = true)]
async fn a_paused_read_resumes_on_its_own_timer() {
    let clock = ManualClock::new();
    let server = server_with(
        "[categories.request]\ndefault = [{ count = 2, window_seconds = 1 }]",
        &clock,
        |config| {
            config.streams.keepalive_interval_ms = 60_000;
            config.streams.poll_interval_ms = 10;
        },
    )
    .await?;
    // A live delivery shows the tailer has passed the envelope, so it sends
    // the paused stream no notice of it later.
    let mut watcher = Native::open(&server).await?;
    applied(&mut watcher, 1, queries(8..9), vec![]).await?;
    clock.advance(secs(1));
    server
        .publish(vec![
            xmtp_mls_validation::test_utils::inline_welcome_envelope(identifier(8)),
        ])
        .await?;
    assert_eq!(watcher.messages(1).await?.len(), 1);
    // Every unit spent so far is earned back.
    clock.advance(secs(1));
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(8..9), vec![]).await?;
    xmtp_common::wait_for_ge(|| async { denials(&server, Category::Request).len() }, 1).await?;
    assert_eq!(
        denials(&server, Category::Request),
        [Duration::from_millis(500)]
    );
    clock.advance(500_000_000);
    let frame = xmtp_common::time::timeout(Duration::from_secs(2), stream.next()).await??;
    assert!(matches!(frame, Frame::Messages(messages) if messages.envelopes.len() == 1));
    drop((watcher, stream));
    server.stop().await?;
}

/// A read paused for topics that are then removed reads nothing, and spends
/// nothing, when it would resume.
// verifies: RATE-033, API-253
#[xmtp_common::test(unwrap_try = true)]
async fn a_paused_read_for_removed_topics_spends_nothing_after_its_delay() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.request]\ndefault = [{ count = 2, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    let metas = server.publish(vec![envelope(9, 0)]).await?;
    clock.advance(secs(60));
    let from = server.rate_limit_spends().len();
    let topic = metas[0].topic.clone().ok_or("topic")?;
    let mut stream = Native::open(&server).await?;
    applied(
        &mut stream,
        1,
        vec![support::query_topic(topic.clone(), 0)],
        vec![],
    )
    .await?;
    xmtp_common::wait_for_ge(|| async { denials(&server, Category::Request).len() }, 1).await?;
    applied(&mut stream, 2, vec![], vec![topic]).await?;
    clock.advance(secs(30));
    ping(&mut stream, 1).await?;
    ping(&mut stream, 2).await?;
    assert_eq!(
        spends(&server, from, Category::Request),
        [(1, true), (1, true), (1, false)]
    );
    drop(stream);
    server.stop().await?;
}

/// A read denied in the middle of a catch-up resumes after its delay with
/// the registrations it covered, less one removed during the pause, and
/// delivers each envelope once.
// verifies: RATE-033, API-253, API-254
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 2)]
async fn a_read_denied_mid_catch_up_resumes_without_a_topic_removed_meanwhile() {
    let clock = ManualClock::new();
    let server = server(
        "[categories.request]\ndefault = [{ count = 4, window_seconds = 60 }]",
        &clock,
    )
    .await?;
    server
        .publish(
            (0..600)
                .map(|index| {
                    xmtp_mls_validation::test_utils::inline_welcome_envelope(identifier(index))
                })
                .collect(),
        )
        .await?;
    clock.advance(secs(60));
    let from = server.rate_limit_spends().len();
    let mut stream = Native::open(&server).await?;
    applied(&mut stream, 1, queries(0..600), vec![]).await?;
    // The `Subscribe`, the target, and two reads of 256 spend all four units.
    let mut rows = stream.messages(512).await?;
    xmtp_common::wait_for_ge(|| async { denials(&server, Category::Request).len() }, 1).await?;
    applied(&mut stream, 2, vec![], vec![topic(599)]).await?;
    clock.advance(secs(15));
    rows.extend(ping_for_messages(&mut stream, 1, 87).await?);
    let delivered: std::collections::HashSet<_> = rows
        .iter()
        .filter_map(|row| row.meta.as_ref()?.topic.clone())
        .collect();
    assert_eq!(rows.len(), 599);
    assert_eq!(delivered.len(), 599);
    assert!(!delivered.contains(&topic(599)));
    assert_eq!(
        spends(&server, from, Category::Request),
        [
            (1, true),
            (1, true),
            (1, true),
            (1, true),
            (1, false),
            (1, true)
        ]
    );
    drop(stream);
    server.stop().await?;
}

/// A static stream renews its rent each period and ends when a renewal is
/// refused.
// verifies: RATE-030, RATE-032
#[xmtp_common::test(unwrap_try = true)]
async fn a_static_stream_renews_its_rent_and_ends_when_refused() {
    use api::subscribe_static_response::Response as Static;
    let clock = ManualClock::new();
    // Each period pays 2 units of 3 per 120 s: the third renewal does not fit.
    let server = server_with(
        "[categories.stream_rent]\ndefault = [{ count = 3, window_seconds = 120 }]",
        &clock,
        |config| config.streams.keepalive_interval_ms = 100,
    )
    .await?;
    let mut stream =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone())
            .subscribe_static(api::SubscribeStaticRequest {
                topics: queries(0..1),
            })
            .await?
            .into_inner();
    assert!(matches!(
        stream.message().await?.and_then(|frame| frame.response),
        Some(Static::Started(_))
    ));
    for periods in 1..=2 {
        clock.advance(secs(60));
        xmtp_common::wait_for_eq(
            || async { spends(&server, 0, Category::StreamRent) },
            vec![(2, true); 1 + periods],
        )
        .await?;
    }
    clock.advance(secs(60));
    let status = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            match stream.message().await {
                Ok(Some(api::SubscribeStaticResponse {
                    response: Some(Static::Keepalive(_)),
                })) => continue,
                Ok(frame) => panic!("unexpected frame {frame:?}"),
                Err(status) => break status,
            }
        }
    })
    .await?;
    assert_eq!(
        rate_limit_delay(&Err::<(), _>(status), Category::StreamRent),
        Some(Duration::from_secs(20))
    );
    server.stop().await?;
}
