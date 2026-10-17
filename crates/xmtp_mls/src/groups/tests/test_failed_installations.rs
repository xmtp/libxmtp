//! An installation the sender cannot add must land in
//! `GroupMembership::failed_installations`: whether its key package is
//! absent from the backend, or becomes unfetchable between intent-build and
//! publish.

use std::collections::HashSet;

use crate::builder::ClientBuilder;
use crate::groups::GroupError;
use crate::groups::intents::QueueIntent;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::group::GroupQueryArgs;
use xmtp_id::associations::builder::SignatureRequestBuilder;
use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{InstallationKeyContext, MemberIdentifier};
use xmtp_mls_validation::commit::extract_group_membership;

/// Requirement: when the backend serves no key package for one of an inbox's
/// installations, the sender adds the inbox with its other installations and
/// records the absent one as failed. Every installation in the inbox is then
/// a leaf or a failed installation, so a joiner accepts the Welcome. Dropping
/// the absent installation silently would make every joiner reject it.
// verifies: JOIN-011, JOIN-012, GMOD-009
#[xmtp_common::test(unwrap_try = true)]
async fn missing_package_is_recorded() {
    tester!(alix);
    tester!(caro);
    let bo_wallet = generate_local_wallet();
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;

    // A sibling installation that joins bo's inbox but never publishes a key package.
    let absent = XmtpInstallationCredential::new();
    let absent_id = absent.public_slice().to_vec();
    let mut request = SignatureRequestBuilder::new(bo.inbox_id())
        .add_association(
            MemberIdentifier::installation(absent_id.clone()),
            bo_wallet.identifier().into(),
        )
        .build();
    let signature = absent.credential_sign::<InstallationKeyContext>(request.signature_text())?;
    request
        .add_signature(
            UnverifiedSignature::new_installation_key(signature, absent.verifying_key()),
            &bo.context.scw_verifier(),
        )
        .await?;
    add_wallet_signature(&mut request, &bo_wallet).await;
    bo.identity_updates()
        .apply_signature_request(request)
        .await?;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;

    let (membership, leaves) = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>((
                extract_group_membership(mls_group.extensions())?,
                mls_group
                    .members()
                    .map(|member| member.signature_key)
                    .collect::<HashSet<_>>(),
            ))
        })
        .await?;
    assert_eq!(
        membership.failed_installations,
        vec![absent_id.clone()],
        "the installation with no key package must be recorded as failed"
    );
    assert!(
        leaves.contains(bo.installation_public_key().as_slice()),
        "bo's sibling with a valid key package must still be added"
    );
    assert!(
        !leaves.contains(&absent_id),
        "the installation with no key package must have no leaf"
    );
    let bo_state = bo
        .identity_updates()
        .get_latest_association_state(&bo.context.db(), bo.inbox_id())
        .await?;
    assert_eq!(bo_state.installation_ids().len(), 2);
    assert!(
        bo_state
            .installation_ids()
            .iter()
            .all(|id| leaves.contains(id) || membership.failed_installations.contains(id)),
        "every installation in bo's identity state must be a leaf or a failed installation"
    );

    for joiner in [&*caro, &bo] {
        joiner.sync_welcomes().await?;
        assert_eq!(
            joiner.find_groups(GroupQueryArgs::default())?.len(),
            1,
            "a joiner must accept a Welcome whose membership accounts for the absent installation"
        );
    }
}

/// `doomed_installation` must belong to an inbox that is already in the
/// group and that just gained an installation. Publish then raises that
/// inbox's sequence id. The membership therefore claims the installation.
async fn publish_update_with_publish_time_failure(
    alix_group: &TestMlsGroup,
    inbox_ids_to_add: &[&str],
    doomed_installation: &[u8],
) -> Result<(), GroupError> {
    // Build the intent while every key package still fetches and verifies.
    let intent_data = alix_group
        .get_membership_update_intent(inbox_ids_to_add, &[])
        .await?;
    assert!(
        !intent_data.is_empty(),
        "the new installation should produce a membership update"
    );
    assert!(
        intent_data.failed_installations.is_empty(),
        "every key package should still be fetchable when the intent is built"
    );

    // The key package now fails: rotation, expiry, or a bad verify.
    set_test_mode_upload_malformed_keypackage(true, Some(vec![doomed_installation.to_vec()]));

    let intent = QueueIntent::update_group_membership()
        .data(intent_data)
        .queue(alix_group)?;
    alix_group.sync_until_intent_resolved(intent.id).await?;

    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn publish_time_key_package_failure_lands_in_membership() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    // bo adds a second installation. This raises bo's identity sequence id.
    tester!(bo2, from: bo);
    let bo2_installation = bo2.context.installation_id().to_vec();

    publish_update_with_publish_time_failure(&alix_group, &[], &bo2_installation).await?;

    let membership = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions())?)
        })
        .await?;

    assert!(
        membership.failed_installations.contains(&bo2_installation),
        "the publish-time key package failure must be recorded in the membership extension"
    );

    // The entry means something only if bo2 has no leaf.
    let leaf_installations = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(
                mls_group
                    .members()
                    .map(|member| member.signature_key)
                    .collect::<Vec<_>>(),
            )
        })
        .await?;
    assert!(
        !leaf_installations.contains(&bo2_installation),
        "bo2 should have no leaf in the ratchet tree"
    );

    set_test_mode_upload_malformed_keypackage(false, None);
}

#[xmtp_common::test(unwrap_try = true)]
async fn joiner_accepts_welcome_with_publish_time_failed_installation() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    tester!(bo2, from: bo);
    let bo2_installation = bo2.context.installation_id().to_vec();

    // The same commit adds caro and fails on bo2. caro's welcome claims bo
    // at a sequence id that holds bo2. But bo2 has no leaf.
    publish_update_with_publish_time_failure(&alix_group, &[caro.inbox_id()], &bo2_installation)
        .await?;

    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(
        caro_groups.len(),
        1,
        "caro's welcome must not be rejected as InvalidGroupMembership"
    );
    assert_eq!(caro.find_groups(GroupQueryArgs::default())?.len(), 1);

    set_test_mode_upload_malformed_keypackage(false, None);
}

// verifies: PROC-036
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn failed_key_package_publish_log_omits_installation_ids() {
    use crate::groups::intents::ProposeMemberUpdateIntentData;
    use std::sync::Arc;
    use tracing::instrument::WithSubscriber;
    use xmtp_db::group_intent::{IntentKind, NewGroupIntent};
    use xmtp_db::prelude::QueryGroupIntent;
    use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};

    #[derive(Default)]
    struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
    impl LogSinkTarget for Capture {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }
    struct RestoreKeyPackages;
    impl Drop for RestoreKeyPackages {
        fn drop(&mut self) {
            set_test_mode_upload_malformed_keypackage(false, None);
        }
    }

    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.sync().await?;
    let failed_id = bo.context.installation_id().to_vec();
    set_test_mode_upload_malformed_keypackage(true, Some(vec![failed_id.clone()]));
    let _restore = RestoreKeyPackages;
    group.context.db().insert_group_intent(NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        group.group_id,
        ProposeMemberUpdateIntentData::new(vec![bo.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;

    let sink = Arc::new(Capture::default());
    let capture = LogCapture::with_sink(Level::Trace, Some(sink.clone()));
    let summary = group
        .sync_with_conn()
        .with_subscriber(capture.dispatch())
        .await
        .expect_err("all requested key packages fail verification");
    let error = summary
        .publish_errors
        .first()
        .expect("publish failure retained");
    let GroupError::FailedToVerifyInstallations(failed) = error else {
        panic!("expected failed key package verification error");
    };
    assert_eq!(failed.0, vec![failed_id.clone()]);
    assert!(error.to_string().contains(&hex::encode(&failed_id)));
    let json = capture.output();
    let records = sink.0.lock();
    assert!(
        records
            .iter()
            .any(|record| record.message.starts_with("Sync: error publishing intents"))
    );
    for sensitive in [hex::encode(&failed_id), format!("{failed_id:?}")] {
        assert!(
            !json.contains(&sensitive),
            "full installation ID reached JSON log"
        );
        for record in records.iter() {
            assert!(
                !record.message.contains(&sensitive),
                "full installation ID reached sink message"
            );
            assert!(
                !format!("{:?}", record.fields).contains(&sensitive),
                "full installation ID reached sink fields"
            );
        }
    }
    let record = records
        .iter()
        .find(|record| record.message.starts_with("Sync: error publishing intents"))
        .expect("automatic publish failure record");
    assert_eq!(record.message, "Sync: error publishing intents");
    assert_eq!(
        record
            .fields
            .get("failed_installation_count")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        record.fields.get("error_kind").map(String::as_str),
        Some("FailedToVerifyInstallations")
    );
}
