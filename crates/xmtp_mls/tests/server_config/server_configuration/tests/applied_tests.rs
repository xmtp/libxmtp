//! Published configuration through a real client.
//!
//! Each applied field is given a non-default value through the static
//! provider, and the behaviour it changes is asserted. A provider
//! short-circuits the fetch, the store, the refresh, and the identifier check,
//! so these tests name values no deployment has to publish and still run
//! against the shared backend.

use std::sync::Arc;
use xmtp_configuration::{
    AttachmentsConfiguration, BACKEND_DEFAULT_MAX_UPLOAD_BYTES, ConfigProvider,
    ServerConfiguration, StaticConfigProvider,
};
use xmtp_mls::context::CoreContext;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_proto::api_client::XmtpBackendClient;

fn provider(edit: impl FnOnce(&mut ServerConfiguration)) -> Arc<dyn ConfigProvider> {
    Arc::new(StaticConfigProvider::edited(edit))
}

fn attachment_offer() -> AttachmentsConfiguration {
    AttachmentsConfiguration {
        base_url: "https://CDN.example.com/att".to_owned(),
        max_upload_bytes: 1024,
        retention_seconds: 0,
    }
}

// A Rust provider bypasses proto conversion, but the client must still hide
// every unusable attachment offer.
// verifies: ATCH-008
#[xmtp_common::test(unwrap_try = true)]
async fn provider_attachment_invalid_offers_are_ignored() {
    xmtp_mls::tester!(invalid_url, config_provider: provider(|configuration| {
        configuration.attachments = Some(AttachmentsConfiguration {
            base_url: "https://x/a/".to_owned(),
            ..attachment_offer()
        });
    }));
    assert!(invalid_url.server_configuration().attachments.is_none());

    xmtp_mls::tester!(invalid_size, config_provider: provider(|configuration| {
        configuration.attachments = Some(AttachmentsConfiguration {
            max_upload_bytes: 4_294_967_296,
            ..attachment_offer()
        });
    }));
    assert!(invalid_size.server_configuration().attachments.is_none());

    xmtp_mls::tester!(invalid_retention, config_provider: provider(|configuration| {
        configuration.attachments = Some(AttachmentsConfiguration {
            retention_seconds: 9_007_199_254_740_992,
            ..attachment_offer()
        });
    }));
    assert!(
        invalid_retention
            .server_configuration()
            .attachments
            .is_none()
    );
}

// A usable offer stays intact, including the published URL string. A zero
// ceiling takes the same compiled default that proto conversion applies.
// verifies: ATCH-008, CONF-025
#[xmtp_common::test(unwrap_try = true)]
async fn provider_attachment_valid_offer_and_default_limit_are_kept() {
    let expected = AttachmentsConfiguration {
        retention_seconds: 9_007_199_254_740_991,
        ..attachment_offer()
    };
    let offered = expected.clone();
    xmtp_mls::tester!(valid_offer, config_provider: provider(move |configuration| {
        configuration.attachments = Some(offered);
    }));
    assert_eq!(
        valid_offer.server_configuration().attachments,
        Some(expected)
    );

    xmtp_mls::tester!(default_limit, config_provider: provider(|configuration| {
        configuration.attachments = Some(AttachmentsConfiguration {
            max_upload_bytes: 0,
            ..attachment_offer()
        });
    }));
    assert_eq!(
        default_limit
            .server_configuration()
            .attachments
            .as_ref()
            .map(|offer| offer.max_upload_bytes),
        Some(BACKEND_DEFAULT_MAX_UPLOAD_BYTES)
    );
}

// The snapshot's ceiling is what the publish path measures against, so
// an envelope above it is refused before any network call.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_envelope_limit_refuses_the_publish() {
    // Large enough for registration, key packages, and the group commit; far
    // below the envelope built at the end of this test.
    xmtp_mls::tester!(alix, config_provider: provider(|c| c.limits.max_envelope_bytes = 65_536));
    let limits = alix.context.api().limits().clone();
    assert_eq!(limits.max_envelope_bytes, 65_536);

    // A message this deployment's ceiling excludes and the compiled default
    // admits, so the refusal below is the deployment's and not the payload's.
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"small enough", SendMessageOpts::default())
        .await?;
    // A keyed group message, signed under the group's own key as a member
    // signs it, so admission refuses it only for its size.
    let mut envelope = xmtp_mls_validation::test_utils::group_message_envelope(
        group.group_id,
        xmtp_mls_validation::test_utils::GroupMessageKind::Application,
        vec![b'x'; 256 * 1024],
    );
    let Some(xmtp_proto::backend_v1::client_envelope::Payload::GroupMessage(message)) =
        envelope.payload.as_mut()
    else {
        unreachable!("the fixture builds a group message");
    };
    group.sign_as_member(message);

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
    xmtp_mls::tester!(alix, config_provider: provider(|c| c.limits.max_query_topics = 1));
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

#[xmtp_common::test(unwrap_try = true)]
async fn an_app_supplied_empty_handle_counts_as_a_credential_source() {
    let mut transport = xmtp_api_backend::MessageBackendBuilder::new();
    transport
        .host(xmtp_configuration::backend_test_url())
        .maybe_auth_handle(Some(xmtp_api_backend::AuthHandle::new()));
    assert!(transport.build()?.has_credential_source());
}
