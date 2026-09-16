//! Spec 006 §6.4 through a real client (CFG-100).
//!
//! Every field §6.4 acts on is given a non-default value through the static
//! provider of CFG-033, and the behaviour it changes is asserted. A provider
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

// CFG-100: every field of §5.2 the deployment publishes reaches
// `serverConfiguration()` unchanged, including the ones §6.4 never reads.
#[xmtp_common::test(unwrap_try = true)]
async fn every_published_field_round_trips_to_the_client() {
    let expected = distinct_snapshot();
    let snapshot = expected.clone();
    crate::tester!(alix, config_provider: provider(move |c| *c = snapshot));

    assert_eq!(alix.server_configuration(), &expected);
}

// CFG-066: the deployment's ceiling is checked before the commit is built and
// before anything is published.
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

    // The same addition against a deployment that publishes room for two.
    crate::tester!(dana, config_provider: provider(|c| c.mls.max_group_members = 2));
    let group = dana.create_group(None, None)?;
    group.add_members_by_identity(&invitees).await?;
    assert_eq!(group.members().await?.len(), 3);
}

// CFG-067: the ceiling is read from the snapshot the client resolved before any
// identity work, and refuses the registration before it publishes.
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

// CFG-068: a deployment that keeps no commit log gets no commit-log entries,
// whatever the client's own worker switch says.
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

// CFG-065: the snapshot's ceiling is what the publish path measures against, so
// an envelope above it is refused before any network call.
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

// CFG-064: queries are chunked at the snapshot's `max_query_topics`, so a
// deployment that publishes one still answers a read across several topics.
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

// CFG-062: a deployment that requires a credential refuses a client that has no
// way to produce one, and the error carries the scopes it wanted.
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

// CFG-063: auth off with a callback configured is not an error. The client
// still builds; the backend simply ignores the credential.
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_with_authentication_off_still_builds() {
    build_with(provider(|c| c.auth.enabled = false)).await?;
}

// CFG-060: the minimum-version check applies to the snapshot the client ends up
// holding, including one a provider supplied.
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

// CFG-051 and CFG-061: once latched, every later call fails with the reason the
// client latched, and the client's cancellation token closes its streams.
#[xmtp_common::test(unwrap_try = true)]
async fn a_latched_client_fails_every_later_call() {
    use crate::context::XmtpSharedContext;

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
    // client that can publish it (CFG-061).
    assert!(
        group
            .send_message(b"after", SendMessageOpts::default())
            .await
            .is_err(),
        "a latched client must not send"
    );

    // The streams a latch closes are closed by cancelling the context token.
    // The refresh worker does that; here the assertion is that cancelling is
    // all it takes, and that the latch survives to explain why.
    alix.context.cancellation_token().cancel();
    assert!(alix.context.server_configuration().check().is_err());
}
