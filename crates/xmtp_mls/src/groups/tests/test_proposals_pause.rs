//! Pause and recovery paths.

use crate::{context::XmtpSharedContext, tester};
use xmtp_db::prelude::*;

/// A Welcome with a higher AppData version floor remains pending without
/// installing the group. An upgraded client can process the same saved input.
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_on_dictionary_group_pauses_below_min_version() {
    use crate::builder::ClientBuilder;
    use crate::groups::tests::increment_patch_version;
    use crate::utils::VersionInfo;
    use xmtp_cryptography::utils::generate_local_wallet;

    // Alix runs at a newer version. Before migrating, she bumps the
    // legacy GMM's `MinimumSupportedProtocolVersion` to her pkg_version
    // so the bootstrap synthesis carries that floor into the AppData
    // dict (synthesis reads `gmm.attributes` to seed dict entries).
    let mut alix_version = VersionInfo::default();
    alix_version.test_update_version(
        increment_patch_version(alix_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let alix_pkg_version = alix_version.pkg_version().to_string();
    let alix =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), alix_version.clone())
            .await;

    // Bo joins PRE-migration so alix's bootstrap synthesis has resolved
    // member identities to work with. He's at the floor version so the
    // pre-migration min-version bump pauses him via legacy GMM — not
    // the subject of this test.
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    // Bump min-version in legacy GMM (the only place to write it
    // before migration). Bootstrap synthesis will pull this value
    // forward into the AppData dict.
    alix_group.update_group_min_version_to_match_self().await?;
    alix_group.sync().await?;

    // Alix migrates. Post-migration the legacy GMM is stripped and the
    // floor lives in the AppData dict only.

    // Carol's Welcome carries the version floor only in the AppData dict.
    tester!(carol, disable_workers);
    alix_group
        .add_members(&[carol.context.identity.inbox_id()])
        .await?;

    let error = carol.sync_welcomes().await.unwrap_err();
    let crate::groups::GroupError::StreamBarrier(error) = error else {
        panic!("expected an unsupported Welcome barrier, got {error:?}");
    };
    let topic = xmtp_proto::types::Topic::new_welcome_message(carol.context.installation_id());
    let status = super::assert_blocked_obligation(
        &error,
        &topic,
        xmtp_proto::types::Cursor(0),
        "welcome_blocked",
    );
    let target = status.target?;
    assert_eq!(status.unresolved_welcomes, vec![target]);
    let db_topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: carol.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let pending = carol.context.db().pending_envelope(&db_topic, target)??;
    assert!(pending.blocked);
    assert!(
        carol
            .context
            .db()
            .find_group(&alix_group.group_id)?
            .is_none()
    );
    assert!(carol.context.db().read_last_rejection(&db_topic)?.is_none());

    let carol = ClientBuilder::from_client(carol.client)
        .version(alix_version)
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(carol.version_info().pkg_version(), alix_pkg_version);
    assert_eq!(
        carol
            .context
            .db()
            .pending_envelope(&db_topic, target)??
            .envelope,
        pending.envelope,
    );
    carol.sync_welcomes().await?;
    let carol_group = carol.group(&alix_group.group_id)?;
    assert!(carol_group.paused_for_version()?.is_none());
    assert_eq!(
        carol_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
    assert!(
        carol
            .context
            .db()
            .pending_envelope(&db_topic, target)?
            .is_none()
    );
}

/// XIP §3 steady-state pause path: when a dictionary-native client bumps
/// `MIN_SUPPORTED_PROTOCOL_VERSION` on a dictionary-native group, the
/// floor flows as an `AppDataUpdate(MIN_SUPPORTED_PROTOCOL_VERSION)`
/// proposal carried inside a regular commit. The legacy GMM extension
/// is absent, so the validator cannot diff it. Instead it
/// must read the post-commit floor from the dict overlay (current dict
/// + any staged AppDataUpdate proposals targeting the component) and
/// raise `ProtocolVersionTooLow` against the receiver's pkg_version.
/// `mls_sync` then writes `paused_for_version`.
///
/// Sibling of `test_welcome_on_dictionary_group_pauses_below_min_version`
/// (welcome-time pause). Pre-fix, the dictionary branch of
/// `ValidatedCommit::from_staged_commit` set
/// `MutableMetadataValidationInfo::default()` unconditionally — so
/// `minimum_supported_protocol_version` was always `None`, the
/// validator's version arm never fired on migrated groups, and a
/// below-floor receiver silently kept processing commits.
#[xmtp_common::test(unwrap_try = true)]
async fn test_steady_state_pause_on_min_version_bump_via_app_data_update() {
    use crate::builder::ClientBuilder;
    use crate::groups::tests::increment_patch_version;
    use crate::utils::VersionInfo;
    use xmtp_cryptography::utils::generate_local_wallet;

    // Alix runs one patch ahead of the default pkg_version so she can
    // legitimately bump the floor to her own version — the send-side
    // clamp added in this change rejects `min_version > own_pkg_version`
    // (footgun guard). Bo stays at the default version so he ends up
    // below the new floor.
    let mut alix_version = VersionInfo::default();
    let bumped = increment_patch_version(alix_version.pkg_version()).expect("patch bump");
    alix_version.test_update_version(&bumped);
    let alix_pkg_version = alix_version.pkg_version().to_string();
    let alix =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), alix_version).await;

    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups
        .iter()
        .find(|g| g.group_id == alix_group.group_id)
        .expect("bo should receive a welcome for alix_group");
    bo_group.sync().await?;

    // The creation floor permits both clients. Raising that committed floor
    // must still pause the client whose version is too low.
    for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
        assert!(
            group.paused_for_version()?.is_none(),
            "{label} must not be paused at the creation floor"
        );
    }

    let before = bo_group.epoch_authenticator().await?;
    let db_topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let processed = bo.context.db().topic_progress(&db_topic)?.processed;

    // Alix raises the floor to her own version, which is above bo's.
    // Send-side clamp is satisfied (alix's pkg_version == requested
    // floor). The bump flows as an
    // `AppDataUpdate(MIN_SUPPORTED_PROTOCOL_VERSION)` inside a commit —
    // The legacy GMM extension is absent, so the dictionary is
    // the only path the floor can ride on.
    alix_group
        .update_group_min_version(&alix_pkg_version)
        .await?;

    let envelopes = alix
        .context
        .api()
        .query_group_messages(alix_group.group_id)
        .await?;
    let added: Vec<_> = envelopes
        .iter()
        .filter(|message| message.cursor > processed)
        .collect();
    let [proposal, commit] = added.as_slice() else {
        panic!("expected the version proposal and its commit, got {added:?}");
    };
    assert_eq!(
        proposal.message.content_type(),
        openmls::prelude::ContentType::Proposal
    );
    assert!(commit.is_commit());
    let predecessor = proposal.cursor;
    let blocked_cursor = commit.cursor;

    super::assert_version_sync_blocked(
        bo_group.sync().await.unwrap_err(),
        &xmtp_proto::types::Topic::new_group_message(bo_group.group_id),
        predecessor,
    );
    assert_eq!(bo_group.epoch_authenticator().await?, before);
    assert_eq!(
        bo.context.db().topic_progress(&db_topic)?.processed,
        predecessor
    );
    let pending = bo.context.db().first_pending_envelope(&db_topic)??;
    assert_eq!(pending.sequence_id as u64, blocked_cursor.0);
    assert!(pending.blocked);
    assert_eq!(
        pending.error_code.as_deref(),
        Some("unsupported_protocol_version")
    );
    assert!(bo.context.db().read_last_rejection(&db_topic)?.is_none());
    let paused = bo_group.paused_for_version()?;
    assert_eq!(
        paused.as_deref(),
        Some(alix_pkg_version.as_str()),
        "bo must be paused at the new floor via the AppDataUpdate-driven path; \
         the legacy GMM extension is absent, so the dict overlay is the \
         only floor signal the validator can read"
    );
}

/// Downgrade safety: pausing normally happens at the floor-*bump* commit
/// (the post-policy `ProtocolVersionTooLow` check), but a client that
/// processed the bump while ABOVE the floor and then downgrades never
/// took that pause — its DB holds a migrated group whose committed
/// floor exceeds its (new, lower) version, with `paused_for_version`
/// unset. The next sync must pause the group from the committed dict
/// floor (the pause-before-parse guards) rather than surface commit
/// processing errors: a non-retryable rejection here is a fork, since
/// above-floor peers accept the same commits.
///
/// Emulates the downgrade by re-opening the same persistent store with
/// a client built at a lower pkg_version, mirroring the restart pattern
/// in `test::builder::identity_persistence_test`. The post-bump commit
/// in this test uses a format both versions understand, so it pins the
/// end state (paused, at the right floor, no error) rather than
/// distinguishing which floor check fired first; the guard ordering
/// (pre-dispatch, before any payload is interpreted) is pinned by the
/// unit tests on `committed_floor_exceeding_in_extensions` plus the
/// guard's placement in `process_message_with_app_data`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_downgraded_client_pauses_on_dictionary_group_with_higher_floor() {
    use crate::builder::ClientBuilder;
    use crate::client::Client;
    use crate::groups::tests::increment_patch_version;
    use crate::identity::IdentityStrategy;
    use crate::utils::{DefaultTestClientCreator, VersionInfo, test::register_client};
    use xmtp_common::tmp_path;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::XmtpTestDb;
    use xmtp_id::InboxOwner;
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    // Both clients start one patch above the default version so the
    // floor can be raised to a value the default version doesn't meet.
    let mut high_version = VersionInfo::default();
    let bumped = increment_patch_version(high_version.pkg_version()).expect("patch bump");
    high_version.test_update_version(&bumped);

    let alix =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), high_version.clone())
            .await;

    // Bo lives on a persistent store so the same identity can be
    // re-opened at a lower version below.
    let bo_wallet = generate_local_wallet();
    let bo_db_path = tmp_path();
    let bo_ident = bo_wallet.get_identifier()?;
    let bo_nonce = 1;
    let bo_inbox_id = bo_ident.inbox_id(bo_nonce)?;
    let bo_strategy = IdentityStrategy::new(bo_inbox_id.clone(), bo_ident.clone(), bo_nonce, None);

    let bo_store = xmtp_db::TestDb::create_persistent_store(Some(bo_db_path.clone())).await;
    let bo = Client::builder(bo_strategy.clone())
        .api_client(DefaultTestClientCreator::create().build()?)
        .store(bo_store)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .version(high_version.clone())
        .build()
        .await?;
    register_client(&bo, &bo_wallet).await;

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups
        .iter()
        .find(|g| g.group_id == alix_group.group_id)
        .expect("bo should receive a welcome for alix_group");
    bo_group.sync().await?;

    // Migrate with the 0.0.0 test floor so nobody pauses at bootstrap.

    bo_group.sync().await?;

    bo_group.sync().await?;

    // Raise the floor to the bumped version. Bo — at that same version —
    // processes the bump WITHOUT pausing: he meets the floor.
    alix_group.update_group_min_version(&bumped).await?;
    bo_group.sync().await?;
    assert!(
        bo_group.paused_for_version()?.is_none(),
        "bo meets the floor pre-downgrade and must not be paused"
    );

    // Traffic lands after the bump; the downgraded client below will
    // meet this commit as the first thing it processes.
    alix_group
        .update_group_name("post-bump name".to_string())
        .await?;

    // The downgrade: re-open bo's store with a client at the DEFAULT
    // (lower) pkg_version.
    let bo_group_id = bo_group.group_id;
    drop(bo);
    let bo_store = xmtp_db::TestDb::create_persistent_store(Some(bo_db_path)).await;
    let bo_downgraded = Client::builder(bo_strategy)
        .api_client(DefaultTestClientCreator::create().build()?)
        .store(bo_store)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .version(VersionInfo::default())
        .build()
        .await?;

    let bo_group = bo_downgraded.group(&bo_group_id)?;
    // The sync may surface the pause as a per-message outcome; the
    // assertion below is the contract, not the sync result.
    let _ = bo_group.sync().await;

    assert_eq!(
        bo_group.paused_for_version()?.as_deref(),
        Some(bumped.as_str()),
        "a downgraded client below the committed floor must pause the group \
         (deferring all commits for post-upgrade reprocessing), never surface \
         processing errors that would advance past peers"
    );
}

/// Pause recovery: a client that's been pinned to `paused_for_version`
/// gets the flag cleared once their `pkg_version` catches up. Without
/// this sweep a paused group could stay paused indefinitely on quiet
/// installations (the per-group `handle_group_paused` re-evaluator only
/// fires when the group is actively synced, and the sync sweep filters
/// out groups with no new server messages).
///
/// Uses direct `set_group_paused` to install the pause flag rather
/// than driving a real cross-version migration: the pause-side flows
/// are pinned by `test_steady_state_pause_on_min_version_bump_via_app_data_update`
/// and friends, so this test focuses on the sweep logic.
#[xmtp_common::test(unwrap_try = true)]
async fn test_unstick_paused_groups_recovers_after_upgrade() {
    use xmtp_db::prelude::*;

    tester!(alix);
    let alix_pkg = alix.version_info().pkg_version().to_string();
    let alix_group = alix.create_group(None, None)?;
    let group_id_typed = &alix_group.group_id;

    // No paused groups initially → sweep is a no-op.
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "no paused groups → sweep must return 0"
    );

    // Pin the floor above the client's own version → sweep stays
    // hands-off (an installation can't unstick itself by reading a
    // floor it can't yet satisfy).
    alix.context
        .db()
        .set_group_paused(group_id_typed, "999.0.0")?;
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "current pkg_version below floor → sweep must NOT unstick"
    );
    assert_eq!(
        alix_group.paused_for_version()?.as_deref(),
        Some("999.0.0"),
        "pause flag must still be set"
    );

    // Pin the floor at or below the client's own version → sweep
    // clears the flag.
    alix.context
        .db()
        .set_group_paused(group_id_typed, &alix_pkg)?;
    assert_eq!(
        alix.unstick_paused_groups().await?,
        1,
        "current pkg_version == floor → sweep must unstick exactly one group"
    );
    assert!(
        alix_group.paused_for_version()?.is_none(),
        "pause flag must be cleared after the sweep"
    );

    // Idempotent: a second sweep on a clean state is a no-op.
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "second sweep on clean state must be a no-op"
    );

    // Lenient on malformed stored bytes — skip that row, don't
    // poison the sweep for everything else.
    alix.context
        .db()
        .set_group_paused(group_id_typed, "not-a-version")?;
    let result = alix.unstick_paused_groups().await;
    assert!(
        result.is_ok(),
        "sweep must succeed even when a row carries unparseable bytes, got {result:?}",
    );
    assert_eq!(
        result.unwrap(),
        0,
        "unparseable rows are skipped, not unstuck"
    );
    assert_eq!(
        alix_group.paused_for_version()?.as_deref(),
        Some("not-a-version"),
        "unparseable pause row must be preserved verbatim"
    );
}

/// `membership_capabilities` reports raw per-installation extension support
/// plus the group context's extension types — generic facts the app filters.
/// This checks that new groups and their installations advertise
/// `AppDataDictionary`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_capabilities() {
    use crate::groups::{InstallationCapabilities, MlsExtensionType};

    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;

    // Check that each installation advertises dictionary support.
    let supports_proposals = |inst: &InstallationCapabilities| {
        inst.capabilities_known
            && inst
                .supported_extensions
                .contains(&MlsExtensionType::AppDataDictionary)
    };

    let caps = alix_group.membership_capabilities().await?;

    // A new group is dictionary-native.
    assert!(
        caps.context_extensions
            .contains(&MlsExtensionType::AppDataDictionary),
        "a new group's context includes AppDataDictionary"
    );

    assert_eq!(caps.members.len(), 3, "alix, bo, and caro");

    // Every member inbox is represented exactly once.
    let reported: std::collections::HashSet<&str> =
        caps.members.iter().map(|m| m.inbox_id.as_str()).collect();
    assert_eq!(reported.len(), caps.members.len(), "no duplicate inboxes");
    for inbox in [alix.inbox_id(), bo.inbox_id(), caro.inbox_id()] {
        assert!(reported.contains(inbox), "capabilities cover {inbox}");
    }

    // Exactly one installation — the local one (alix's) — is flagged is_own.
    let own_count = caps
        .members
        .iter()
        .flat_map(|m| &m.installations)
        .filter(|i| i.is_own)
        .count();
    assert_eq!(own_count, 1, "only the local installation is marked is_own");

    // All current-code installations advertise AppDataDictionary.
    for member in &caps.members {
        assert!(
            !member.installations.is_empty(),
            "{} should have at least one installation",
            member.inbox_id
        );
        for inst in &member.installations {
            assert!(
                inst.capabilities_known,
                "capabilities known for {}",
                member.inbox_id
            );
            assert!(!inst.installation_id.is_empty());
            assert!(
                supports_proposals(inst),
                "{} advertises AppDataDictionary",
                member.inbox_id
            );
        }
    }

    // All current-code installations advertise dictionary support.
    let blocking: Vec<&str> = caps
        .members
        .iter()
        .filter(|m| m.installations.iter().any(|i| !supports_proposals(i)))
        .map(|m| m.inbox_id.as_str())
        .collect();
    assert!(
        blocking.is_empty(),
        "no inbox lacks dictionary support: {blocking:?}"
    );

    let current = alix_group.membership_capabilities().await?;
    assert!(
        current
            .context_extensions
            .contains(&MlsExtensionType::AppDataDictionary),
        "context advertises AppDataDictionary"
    );
    assert_eq!(current.members.len(), 3);
}
