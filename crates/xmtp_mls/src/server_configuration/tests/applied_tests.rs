//! Published configuration through a real client.
//!
//! Each applied field is given a non-default value through the static
//! provider, and the behaviour it changes is asserted. A provider
//! short-circuits the fetch, the store, the refresh, and the identifier check,
//! so these tests name values no deployment has to publish and still run
//! against the shared backend.

use std::sync::Arc;

use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::XmtpTestDb;
use xmtp_db::prelude::*;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

use crate::Client;
use crate::builder::ClientBuilderError;
use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::utils::DefaultTestClientCreator;
use crate::utils::test::identity_setup;

use super::distinct_snapshot;

fn provider(edit: impl FnOnce(&mut ServerConfiguration)) -> Arc<dyn ConfigProvider> {
    Arc::new(StaticConfigProvider::edited(edit))
}

/// Build against the shared backend with a caller-supplied snapshot, without
/// unwrapping: these cases are about builds that must fail.
async fn build_with(provider: Arc<dyn ConfigProvider>) -> Result<(), ClientBuilderError> {
    let owner = generate_local_wallet();
    Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider)
        .default_mls_store()
        .unwrap()
        .build()
        .await
        .map(|_| ())
}

// Every field the deployment publishes reaches `serverConfiguration()` unchanged.
// verifies: CONF-061
#[xmtp_common::test(unwrap_try = true)]
async fn every_published_field_round_trips_to_the_client() {
    let expected = distinct_snapshot();
    let snapshot = expected.clone();
    crate::tester!(alix, config_provider: provider(move |c| *c = snapshot));

    assert_eq!(alix.server_configuration(), &expected);
}

// The deployment's ceiling is checked before the commit is built and
// before anything is published.
// verifies: CONF-043
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_group_member_limit_refuses_the_addition() {
    crate::tester!(bo);
    crate::tester!(caro);
    let invitees = [bo.identifier(), caro.identifier()];

    // A deployment that publishes room for one.
    crate::tester!(alix, config_provider: provider(|c| c.mls.max_group_members = 1));
    let group = alix.create_group(None, None)?;
    let error = group.add_members_by_identity(&invitees).await.unwrap_err();
    assert!(
        matches!(error, GroupError::UserLimitExceeded),
        "expected the user-limit error, got {error}"
    );

    // The commit was never built, so nobody joined.
    assert!(
        group.members().await?.len() < 2,
        "a refused addition must not reach the commit"
    );

    // Room for two counts the creator, who sits at sequence id zero until the
    // first commit and so is invisible to `members()`. Two invitees would make
    // three, and are refused; one fits exactly.
    crate::tester!(dana, config_provider: provider(|c| c.mls.max_group_members = 2));
    let group = dana.create_group(None, None)?;
    let error = group.add_members_by_identity(&invitees).await.unwrap_err();
    assert!(
        matches!(error, GroupError::UserLimitExceeded),
        "the creator must count against the ceiling, got {error}"
    );
    group.add_members_by_identity(&invitees[..1]).await?;
    assert_eq!(group.members().await?.len(), 2);

    // The inbox-id API enforces the same ceiling: it is the method both entry
    // points reach, so it cannot be used to step over the limit.
    let error = group
        .add_members(&[caro.inbox_id()])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("limit"),
        "the direct inbox-id API must enforce the ceiling too, got {error}"
    );

    // A deployment with room for three admits both.
    crate::tester!(eve, config_provider: provider(|c| c.mls.max_group_members = 3));
    let group = eve.create_group(None, None)?;
    group.add_members_by_identity(&invitees).await?;
    assert_eq!(group.members().await?.len(), 3);
}

// The ceiling is read from the snapshot the client resolved before any
// identity work, and refuses the registration before it publishes.
// verifies: CONF-044
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_installation_limit_refuses_the_registration() {
    // One installation exists after this build.
    crate::tester!(first);
    let owner = first.builder.owner.clone();
    assert_eq!(first.inbox_state(true).await?.installations().len(), 1);

    // A second installation for the same inbox, against a deployment that
    // allows exactly one.
    let error = Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build()?)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider(|c| c.mls.max_installations_per_inbox = 1))
        .default_mls_store()?
        .build()
        .await
        .map(|_| ())
        .expect_err("a second installation must be refused");
    assert!(
        format!("{error}").contains("installations"),
        "expected the installation-limit error, got {error}"
    );

    // Nothing was published: the inbox still has one installation.
    assert_eq!(first.inbox_state(true).await?.installations().len(), 1);
}

// A deployment that keeps no commit log gets no commit-log entries,
// whatever the client's own worker switch says.
// verifies: CONF-045
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_that_keeps_no_commit_log_writes_none() {
    crate::tester!(alix, config_provider: provider(|c| c.mls.commit_log_enabled = Some(false)));
    crate::tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members_by_identity(&[bo.identifier()]).await?;
    group.sync().await?;
    assert!(
        alix.context
            .db()
            .get_group_logs(&group.group_id)?
            .is_empty(),
        "a deployment with the commit log off must write no entries"
    );

    // The same work against a deployment that keeps one does write entries, so
    // the assertion above is about the snapshot and not about the test setup.
    crate::tester!(cassie, config_provider: provider(|c| c.mls.commit_log_enabled = Some(true)));
    let group = cassie.create_group(None, None)?;
    group.add_members_by_identity(&[bo.identifier()]).await?;
    group.sync().await?;
    assert!(
        !cassie
            .context
            .db()
            .get_group_logs(&group.group_id)?
            .is_empty(),
        "a deployment with the commit log on must write entries"
    );
}

// The snapshot's ceiling is what the publish path measures against, so
// an envelope above it is refused before any network call.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_envelope_limit_refuses_the_publish() {
    // Large enough for registration, key packages, and the group commit; far
    // below the envelope built at the end of this test.
    crate::tester!(alix, config_provider: provider(|c| c.limits.max_envelope_bytes = 65_536));
    let limits = alix.context.api().limits().clone();
    assert_eq!(limits.max_envelope_bytes, 65_536);

    // A message this deployment's ceiling excludes and the compiled default
    // admits, so the refusal below is the deployment's and not the payload's.
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"small enough", SendMessageOpts::default())
        .await?;
    let envelope = xmtp_mls_validation::test_utils::group_message_envelope(
        group.group_id,
        xmtp_mls_validation::test_utils::GroupMessageKind::Application,
        vec![b'x'; 256 * 1024],
    );

    let error = xmtp_api::PublishUnit::single_within(envelope.clone(), &limits).unwrap_err();
    assert!(
        matches!(error, xmtp_api::ApiError::EnvelopeTooLarge),
        "expected the envelope-too-large error, got {error}"
    );

    // The same envelope fits the compiled default, so the refusal above is the
    // deployment's ceiling and not the payload.
    xmtp_api::PublishUnit::single(envelope)?;
}

// Queries are chunked at the snapshot's `max_query_topics`, so a
// deployment that publishes one still answers a read across several topics.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_query_topic_limit_chunks_rather_than_truncates() {
    crate::tester!(alix, config_provider: provider(|c| c.limits.max_query_topics = 1));
    assert_eq!(alix.context.api().limits().max_query_topics, 1);

    let mut expected = Vec::new();
    let mut cursors = std::collections::HashMap::new();
    for _ in 0..3 {
        let group = alix.create_group(None, None)?;
        group
            .send_message(b"hello", SendMessageOpts::default())
            .await?;
        let topic = xmtp_proto::types::Topic::new_group_message(group.group_id);
        expected.push(topic.clone());
        cursors.insert(topic, xmtp_proto::types::Cursor(0));
    }

    let limit = alix.context.api().limits().max_query_limit as u32;
    let envelopes = alix.context.api().query_all(cursors, limit).await?;
    assert!(
        envelopes.len() >= expected.len(),
        "a one-topic chunk limit must still read every topic, got {} envelopes",
        envelopes.len()
    );
}

// A deployment that requires a credential refuses a client that has no
// way to produce one, and the error carries the scopes it wanted.
// verifies: CONF-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_requiring_authentication_refuses_a_client_with_no_credential_source() {
    let error = build_with(provider(|c| {
        c.auth.enabled = true;
        c.auth.required_scopes = vec!["xmtp:write".to_owned()];
    }))
    .await
    .expect_err("a deployment requiring authentication must refuse this client");
    let ClientBuilderError::ClientError(crate::client::ClientError::AuthRequired {
        required_scopes,
    }) = error
    else {
        panic!("expected AuthRequired, got {error}");
    };
    assert_eq!(required_scopes, vec!["xmtp:write".to_owned()]);
}

// Auth off with a callback configured is not an error. The client
// still builds; the backend simply ignores the credential.
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_with_authentication_off_still_builds() {
    build_with(provider(|c| c.auth.enabled = false)).await?;
}

// The minimum-version check applies to the snapshot the client ends up
// holding, including one a provider supplied.
// verifies: CONF-049
#[xmtp_common::test(unwrap_try = true)]
async fn a_provider_snapshot_requiring_a_newer_client_refuses_the_build() {
    let error = build_with(provider(|c| {
        c.min_libxmtp_version = "9999.0.0".to_owned();
    }))
    .await
    .expect_err("a client below the published minimum must be refused");
    assert!(
        format!("{error}").contains("9999.0.0"),
        "unexpected error: {error}"
    );
}

// Every signature request the client hands back is bound to the chains
// the deployment accepts, revocation included. Installing the default remote
// verifier also ends the app-supplied-verifier exemption, so a caller that sets
// its own verifier and then asks for the remote one is bound like anyone else.
#[xmtp_common::test(unwrap_try = true)]
async fn a_revocation_request_is_bound_to_the_accepted_chains() {
    let owner = generate_local_wallet();
    let client = Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_remote_verifier()?
        .with_disable_workers(true)
        .config_provider(provider(|c| {
            c.smart_contract_wallet_chains = vec!["eip155:1".to_owned()];
        }))
        .default_mls_store()?
        .build()
        .await?;
    crate::utils::test::register_client(&client, &owner).await;

    let request = client
        .identity_updates()
        .revoke_installations(vec![vec![9u8; 32]])
        .await?;
    assert_eq!(
        request.accepted_chains(),
        Some(&["eip155:1".to_owned()][..]),
        "a revocation request must carry the deployment's chains"
    );
}

// Once latched, every later call fails with the reason the
// client latched, and the client's cancellation token closes its streams.
// verifies: CONF-022
#[xmtp_common::test(unwrap_try = true)]
async fn a_latched_client_fails_every_later_call() {
    use crate::context::XmtpSharedContext;
    use xmtp_common::StreamHandle;

    crate::tester!(alix, config_provider: provider(|_| {}));
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"before", SendMessageOpts::default())
        .await?;

    alix.context
        .server_configuration()
        .latch(super::ConfigurationLatch::ClientVersionTooOld {
            client: "1.0.0".to_owned(),
            minimum: "9999.0.0".to_owned(),
        });

    // The gates that report the latch verbatim: group sync and the client-level
    // readiness check every operation passes through.
    for error in [
        group.sync().await.unwrap_err().to_string(),
        alix.create_group(None, None).unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("9999.0.0"),
            "a latched client must report the latch, got {error}"
        );
    }

    // Sending still fails; the sync driver reports it as a publish failure
    // rather than re-raising the latch, because the intent stays queued for a
    // client that can publish it.
    assert!(
        group
            .send_message(b"after", SendMessageOpts::default())
            .await
            .is_err(),
        "a latched client must not send"
    );

    // The streams a latch closes are closed by cancelling the context token.
    // The refresh worker does that; here the assertion is that cancelling is
    // all it takes, that a callback stream reports the reason rather than
    // closing silently, and that the latch survives to explain why.
    let reported = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let sink = reported.clone();
    let mut handle = crate::Client::stream_consent_with_callback(
        Arc::new((*alix).clone()),
        move |update| {
            if let Err(error) = update {
                sink.lock().push(error.to_string());
            }
        },
        || {},
    );
    handle.wait_for_ready().await;

    alix.context.cancellation_token().cancel();
    assert!(alix.context.server_configuration().check().is_err());

    let closed = handle
        .join()
        .await?
        .expect_err("a latched client must close its streams with the reason");
    assert!(
        closed.to_string().contains("9999.0.0"),
        "a closing stream must report the latch, got {closed}"
    );
    let reported = reported.lock().clone();
    assert!(
        reported.iter().any(|error| error.contains("9999.0.0")),
        "the callback must see the latch, got {reported:?}"
    );
}
